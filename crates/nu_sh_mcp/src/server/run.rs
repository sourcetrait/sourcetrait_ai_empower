use crate::*;

pub fn run_server() {
    let rt = tk::Runtime::new().expect("tokio Runtime::new");
    rt.block_on(async {
        let worker = WorkerHandle::spawn().await.expect("spawn worker");
        let nonce_gen = Arc::new(lib_empower::NonceGen::new());
        let server = NuSh::new(worker, nonce_gen);
        let service = server.serve(mcp::stdio()).await.expect("serve stdio");
        service.waiting().await.expect("service waiting");
    });
}
