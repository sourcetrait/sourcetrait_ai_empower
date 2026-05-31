use crate::*;

use mcp::ServiceExt as _;

pub fn run_server() {
    let rt = tk::Runtime::new().expect("tokio Runtime::new");
    rt.block_on(async {
        let worker = WorkerHandle::spawn().await.expect("spawn worker");
        let server = NuSh::new(worker);
        let service = server.serve(mcp::stdio()).await.expect("serve stdio");
        service.waiting().await.expect("service waiting");
    });
}
