use std::future::Future;

pub struct AsyncRuntime {
    runtime: Option<tokio::runtime::Runtime>,
}

impl AsyncRuntime {
    pub fn new() -> anyhow::Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_io()
            .enable_time()
            .build()?;
        Ok(Self {
            runtime: Some(runtime),
        })
    }

    pub fn spawn<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        if let Some(rt) = &self.runtime {
            rt.spawn(future);
        }
    }

    pub fn handle(&self) -> Option<tokio::runtime::Handle> {
        self.runtime.as_ref().map(|rt| rt.handle().clone())
    }
}

impl Drop for AsyncRuntime {
    fn drop(&mut self) {
        if let Some(rt) = self.runtime.take() {
            rt.shutdown_background();
        }
    }
}
