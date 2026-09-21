use crate::config::{tier, Config, ASSETS};
use crate::state::{DlRow, DlState, Shared};
use anyhow::{bail, Result};
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

/// No bytes for this long counts as a stall.
const STALL: Duration = Duration::from_secs(30);
const ATTEMPTS: u32 = 3;

struct Item {
    id: String,
    name: String,
    url: String,
    dest: PathBuf,
    size: u64,
    unzip: bool,
}

fn plan(cfg: &Config) -> Vec<Item> {
    let t = tier(&cfg.tier);
    let mut v = vec![Item {
        id: t.id.to_string(),
        name: format!("Model {}", t.label),
        url: t.url.to_string(),
        dest: cfg.models_dir().join(t.file),
        size: t.size,
        unzip: false,
    }];
    for a in ASSETS {
        v.push(Item {
            id: a.id.to_string(),
            name: a.name.to_string(),
            url: a.url.to_string(),
            dest: if a.unzip {
                cfg.bin_dir().join(a.file)
            } else {
                cfg.models_dir().join(a.file)
            },
            size: a.size,
            unzip: a.unzip,
        });
    }
    v
}

fn set(state: &Shared, id: &str, f: impl FnOnce(&mut DlRow)) {
    if let Some(r) = state
        .downloads
        .lock()
        .unwrap()
        .iter_mut()
        .find(|r| r.id == id)
    {
        f(r);
    }
}

/// sha256 published by Hugging Face. GitHub release assets have none, so this is optional.
async fn expected_sha(client: &reqwest::Client, url: &str) -> Option<String> {
    let r = client.head(url).send().await.ok()?;
    let v = r.headers().get("x-linked-etag")?.to_str().ok()?;
    Some(v.trim_matches('"').to_lowercase())
}

async fn fetch(state: &Shared, item: &Item, client: &reqwest::Client) -> Result<()> {
    if let Some(dir) = item.dest.parent() {
        std::fs::create_dir_all(dir)?;
    }

    // Already complete from an earlier run?
    if item.dest.metadata().map(|m| m.len()).ok() == Some(item.size) {
        set(state, &item.id, |r| {
            r.got = item.size;
            r.state = DlState::Done;
            r.note.clear();
        });
        return Ok(());
    }

    let sha = expected_sha(client, &item.url).await;

    for attempt in 1..=ATTEMPTS {
        let have = item.dest.metadata().map(|m| m.len()).unwrap_or(0);
        if have > item.size {
            std::fs::remove_file(&item.dest).ok();
        }
        let have = item.dest.metadata().map(|m| m.len()).unwrap_or(0);

        set(state, &item.id, |r| {
            r.got = have;
            r.state = DlState::Connecting;
            r.note.clear();
        });
        state.emit().await;

        let mut req = client.get(&item.url);
        if have > 0 {
            req = req.header("Range", format!("bytes={have}-"));
        }
        let resp = match req.send().await {
            Ok(r) => r,
            Err(_) => {
                set(state, &item.id, |r| {
                    r.state = DlState::Stalled;
                    r.note = format!("stalled · retrying ({attempt}/{ATTEMPTS})");
                });
                state.emit().await;
                tokio::time::sleep(Duration::from_secs(3 * attempt as u64)).await;
                continue;
            }
        };
        if !resp.status().is_success() {
            let code = resp.status().as_u16();
            set(state, &item.id, |r| {
                r.state = DlState::Failed;
                r.note = format!("failed · server error {code}");
            });
            state.emit().await;
            bail!("server error {code}");
        }

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(have > 0)
            .write(true)
            .open(&item.dest)?;
        let mut got = have;
        let mut stream = resp.bytes_stream();
        let mut stalled = false;

        set(state, &item.id, |r| r.state = DlState::Downloading);

        loop {
            match tokio::time::timeout(STALL, stream.next()).await {
                Err(_) => {
                    stalled = true;
                    break;
                }
                Ok(None) => break,
                Ok(Some(Err(_))) => {
                    stalled = true;
                    break;
                }
                Ok(Some(Ok(chunk))) => {
                    file.write_all(&chunk)?;
                    got += chunk.len() as u64;
                    set(state, &item.id, |r| r.got = got);
                    if *state.paused.lock().unwrap() {
                        file.flush()?;
                        set(state, &item.id, |r| {
                            r.state = DlState::Paused;
                            r.note = "paused".into();
                        });
                        state.emit().await;
                        return Ok(());
                    }
                }
            }
        }
        file.flush()?;
        drop(file);

        if stalled {
            set(state, &item.id, |r| {
                r.state = DlState::Stalled;
                r.note = format!("stalled · retrying ({attempt}/{ATTEMPTS})");
            });
            state.emit().await;
            tokio::time::sleep(Duration::from_secs(3 * attempt as u64)).await;
            continue;
        }

        let on_disk = item.dest.metadata()?.len();
        if on_disk != item.size {
            set(state, &item.id, |r| {
                r.state = DlState::Stalled;
                r.note = "stalled · retrying".into();
            });
            state.emit().await;
            continue;
        }

        if let Some(want) = &sha {
            set(state, &item.id, |r| {
                r.state = DlState::Verifying;
                r.note = "verifying".into();
            });
            state.emit().await;
            let path = item.dest.clone();
            let got_sha =
                tokio::task::spawn_blocking(move || -> Result<String> {
                    let mut f = std::fs::File::open(&path)?;
                    let mut h = Sha256::new();
                    std::io::copy(&mut f, &mut h)?;
                    Ok(hex::encode(h.finalize()))
                })
                .await??;
            if &got_sha != want {
                // A bad checksum means the bytes are wrong, not incomplete: start over.
                std::fs::remove_file(&item.dest).ok();
                set(state, &item.id, |r| {
                    r.got = 0;
                    r.state = DlState::Failed;
                    r.note = "failed · checksum didn't match".into();
                });
                state.emit().await;
                bail!("checksum mismatch");
            }
        }

        if item.unzip {
            set(state, &item.id, |r| {
                r.state = DlState::Unpacking;
                r.note = "unpacking".into();
            });
            state.emit().await;
            let zip = item.dest.clone();
            let into = item.dest.parent().unwrap().to_path_buf();
            tokio::task::spawn_blocking(move || unpack(&zip, &into)).await??;
            std::fs::remove_file(&item.dest).ok();
        }

        set(state, &item.id, |r| {
            r.got = item.size;
            r.state = DlState::Done;
            r.note.clear();
        });
        state.emit().await;
        return Ok(());
    }

    set(state, &item.id, |r| {
        r.state = DlState::Failed;
        r.note = format!("failed · stalled {ATTEMPTS} times");
    });
    state.emit().await;
    bail!("gave up after {ATTEMPTS} attempts");
}

/// Flattens the archive: sd.cpp ships its DLLs and exes at the top level already,
/// but the CUDA runtime zip nests them one folder deep.
fn unpack(zip_path: &PathBuf, into: &PathBuf) -> Result<()> {
    let file = std::fs::File::open(zip_path)?;
    let mut z = zip::ZipArchive::new(file)?;
    for i in 0..z.len() {
        let mut e = z.by_index(i)?;
        if e.is_dir() {
            continue;
        }
        let name = e
            .enclosed_name()
            .and_then(|p| p.file_name().map(|f| f.to_owned()))
            .ok_or_else(|| anyhow::anyhow!("bad zip entry"))?;
        let out = into.join(name);
        let mut f = std::fs::File::create(&out)?;
        std::io::copy(&mut e, &mut f)?;
    }
    Ok(())
}

pub async fn install(state: Shared) {
    if *state.installing.lock().unwrap() {
        return;
    }
    *state.installing.lock().unwrap() = true;
    *state.paused.lock().unwrap() = false;

    let cfg = state.cfg.lock().await.clone();
    let items = plan(&cfg);

    *state.downloads.lock().unwrap() = items
        .iter()
        .map(|i| DlRow {
            id: i.id.clone(),
            name: i.name.clone(),
            got: 0,
            total: i.size,
            state: DlState::Pending,
            note: String::new(),
        })
        .collect();
    state.emit().await;

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .build()
        .unwrap_or_default();

    let mut set_js = tokio::task::JoinSet::new();
    for item in items {
        let st = state.clone();
        let cl = client.clone();
        set_js.spawn(async move {
            let _ = fetch(&st, &item, &cl).await;
        });
    }
    while set_js.join_next().await.is_some() {}

    *state.installing.lock().unwrap() = false;
    state.emit().await;
}

pub async fn retry(state: Shared, id: String) {
    let cfg = state.cfg.lock().await.clone();
    let Some(item) = plan(&cfg).into_iter().find(|i| i.id == id) else {
        return;
    };
    let client = reqwest::Client::new();
    let _ = fetch(&state, &item, &client).await;
    state.emit().await;
}
