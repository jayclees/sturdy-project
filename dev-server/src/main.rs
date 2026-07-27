use notify::Watcher;
use notify_types::event::EventKind;
use std::env;
use std::net::ToSocketAddrs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::Mutex;
use watchexec::action::ActionHandler;
use watchexec::Watchexec;
use watchexec_events::Tag;
use watchexec_signals::Signal;
use watchexec_supervisor::command::{Command, Program};
use watchexec_supervisor::job::start_job;

#[tokio::main]
async fn main() {
    // todo Possibly refactor `cargo run` into separate service and only
    // todo run `cargo build` here. This will allow us to keep the app
    // todo server running while build is running. We can watch the
    // todo target/debug/binary file for changes, and restart
    // todo the app server with near zero downtime.
    let host = resolve_host_url();
    let vite_url = resolve_vite_url();
    let dev_server_root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let app_root = dev_server_root.join("..").canonicalize().unwrap();
    let (job, task) = start_job(Arc::new(Command {
        program: Program::Exec {
            prog: "/usr/bin/bash".into(),
            args: vec![
                "-c".to_owned(),
                format!(
                    "cd {} && cargo run -- --host={} --port=3000 --vite-url={}",
                    app_root.display(),
                    host,
                    vite_url
                ),
            ],
        }
            .into(),
        options: Default::default(),
    }));
    let last_reloaded = Arc::new(Mutex::new(SystemTime::now()));
    let job = Arc::new(job);
    job.start().await;
    let job2 = Arc::clone(&job);
    let wx = Watchexec::new_async(move |mut action: ActionHandler| {
        let job3 = Arc::clone(&job2);
        let last_reloaded = Arc::clone(&last_reloaded);
        Box::new(async move {
            for event in action.events.iter() {
                let path_result = event.tags.iter().find(|tag: &&Tag| {
                    if let Tag::Path { .. } = tag {
                        return true;
                    };
                    false
                });
                let kind_result = event.tags.iter().find(|tag: &&Tag| {
                    if let Tag::FileEventKind(kind) = tag {
                        return match kind {
                            EventKind::Create(_)
                            | EventKind::Modify(_)
                            | EventKind::Remove(_)
                            | EventKind::Other => true,
                            _ => false,
                        };
                    };
                    false
                });
                if let Some(path_outer) = path_result
                    && let Tag::Path { path, .. } = path_outer
                    && let Some(kind_outer) = kind_result
                    && let Tag::FileEventKind(kind) = kind_outer
                {
                    // Filter for change to .rs files in ../src/
                    let path = path.to_str().unwrap();
                    if path.ends_with(".rs") || path.contains("resource/template") {
                        let arc = last_reloaded.clone();
                        let mut time = arc.lock().await;
                        let elapsed = time.elapsed().expect("Failed to get elapsed time");
                        if elapsed > Duration::new(0, 1000000 * 10) {
                            dbg!(&path);
                            *time = SystemTime::now();
                            let r#type = match kind {
                                EventKind::Create(_) => "Create",
                                EventKind::Modify(_) => "Modify",
                                EventKind::Remove(_) => "Remove",
                                EventKind::Other => "Other",
                                _ => unimplemented!("Should not hit."),
                            };

                            // Should restart app server
                            println!("Event occurred: {type}: {path:?}");

                            println!("Stopping cargo run in project root...");
                            job3.stop().await;
                            // ...

                            println!("Starting cargo run in project root...");
                            job3.start().await;
                            // ...
                        }
                    }
                }
            }

            // If Ctrl-C is received, quit.
            // Important: do not remove otherwise you will not be able to quit
            let stop_signal = action.signals().find(|sig| match sig {
                Signal::ForceStop
                | Signal::Interrupt
                | Signal::Quit
                | Signal::Terminate
                | Signal::Custom(_) => true,
                _ => false,
            });
            if stop_signal.is_some() {
                println!("Gracefully shutting down...");
                job3.stop().await;
                action.quit_gracefully(stop_signal.unwrap(), Duration::from_millis(250));
            }

            action
        })
    })
        .unwrap();

    wx.config.pathset([app_root]);
    wx.main().await.unwrap().unwrap();

    job.delete_now().await;
    task.await.unwrap(); // Make sure the task is fully cleaned up
}

/// If running in Docker container, bind to 0.0.0.0, else bind to 127.0.0.1.
fn resolve_host_url() -> String {
    if let Ok(_) = env::var("IS_DOCKER") {
        "0.0.0.0".to_owned()
    } else {
        "127.0.0.1".to_owned()
    }
}

/// Attempt to resolve the Vite server in Docker network, if it
/// fails, assume 127.0.0.1:5173 which is the Vite default.
fn resolve_vite_url() -> String {
    let addrs_iter = "node:5173".to_socket_addrs();
    let vite_url = if let Ok(mut iter) = addrs_iter {
        iter.next().unwrap().to_string()
    } else {
        "127.0.0.1:5173".to_owned()
    };
    vite_url
}
