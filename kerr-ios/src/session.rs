use std::sync::Arc;
use tokio::sync::Mutex;
use crate::{KerrError, FileBrowser, ShellSession, ShellCallback};

pub struct Session {
    conn: Arc<iroh::endpoint::Connection>,
    file_browser: Arc<Mutex<Option<Arc<FileBrowser>>>>,
    shell_session: Arc<Mutex<Option<Arc<ShellSession>>>>,
    connected: Arc<Mutex<bool>>,
}

impl Session {
    pub async fn new(conn: iroh::endpoint::Connection) -> Result<Arc<Self>, KerrError> {
        Ok(Arc::new(Self {
            conn: Arc::new(conn),
            file_browser: Arc::new(Mutex::new(None)),
            shell_session: Arc::new(Mutex::new(None)),
            connected: Arc::new(Mutex::new(true)),
        }))
    }

    pub fn file_browser(self: Arc<Self>) -> Result<Arc<FileBrowser>, KerrError> {
        let runtime = crate::get_runtime();
        runtime.block_on(async {
            let mut fb_lock = self.file_browser.lock().await;

            // Return existing file browser if already created
            if let Some(fb) = fb_lock.as_ref() {
                return Ok(Arc::clone(fb));
            }

            // Create new file browser
            let fb = FileBrowser::new(Arc::clone(&self.conn)).await?;
            *fb_lock = Some(Arc::clone(&fb));
            Ok(fb)
        })
    }

    pub fn start_shell(
        self: Arc<Self>,
        callback: Box<dyn ShellCallback>,
    ) -> Result<Arc<ShellSession>, KerrError> {
        let runtime = crate::get_runtime();
        runtime.block_on(async {
            let mut shell_lock = self.shell_session.lock().await;

            // Close existing shell if any
            if let Some(existing) = shell_lock.take() {
                existing.close();
            }

            // Create new shell session
            let shell = ShellSession::new(Arc::clone(&self.conn), callback).await?;
            *shell_lock = Some(Arc::clone(&shell));
            Ok(shell)
        })
    }

    pub fn disconnect(&self) {
        let runtime = crate::get_runtime();
        runtime.block_on(async {
            let mut connected = self.connected.lock().await;
            *connected = false;

            // Close shell if active
            if let Some(shell) = self.shell_session.lock().await.take() {
                shell.close();
            }

            // Close connection
            self.conn.close(0u32.into(), b"disconnect");
        });
    }

    pub fn is_connected(&self) -> bool {
        let runtime = crate::get_runtime();
        runtime.block_on(async {
            *self.connected.lock().await
        })
    }
}
