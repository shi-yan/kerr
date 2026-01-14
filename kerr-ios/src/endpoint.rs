use std::sync::Arc;
use tokio::sync::Mutex;
use crate::{KerrError, Session, decode_addr, encode_addr, ALPN};

pub struct Endpoint {
    inner: Arc<iroh::endpoint::Endpoint>,
}

impl Endpoint {
    pub async fn new() -> Result<Arc<Self>, KerrError> {
        let endpoint = iroh::endpoint::Endpoint::bind()
            .await
            .map_err(|e| KerrError::ConnectionFailed(e.to_string()))?;

        Ok(Arc::new(Self {
            inner: Arc::new(endpoint),
        }))
    }

    pub fn connect(self: Arc<Self>, connection_string: String) -> Result<Arc<Session>, KerrError> {
        let runtime = crate::get_runtime();
        runtime.block_on(async {
            // Decode connection string
            let addr = decode_addr(&connection_string)?;

            // Connect to the remote
            let conn = self
                .inner
                .connect(addr, ALPN)
                .await
                .map_err(|e| KerrError::ConnectionFailed(e.to_string()))?;

            // Create session
            Session::new(conn).await
        })
    }

    pub fn connection_string(&self) -> Result<String, KerrError> {
        let runtime = crate::get_runtime();
        runtime.block_on(async {
            let node_id = self.inner.node_id();
            let local_addrs: Vec<_> = self
                .inner
                .local_endpoints()
                .await
                .map_err(|e| KerrError::NetworkError(e.to_string()))?
                .into_iter()
                .map(|endpoint| endpoint.addr)
                .collect();

            let addr = iroh::endpoint::NodeAddr::new(node_id)
                .with_direct_addresses(local_addrs);

            encode_addr(&addr)
        })
    }
}
