use crate::*;

pub(crate) fn serve(_warm_base: &WarmBase) -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdin_lock = stdin.lock();
    let mut stdout_lock = stdout.lock();
    loop {
        let frame = match read_frame(&mut stdin_lock) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(e) => return Err(e),
        };
        let req: RunRequest = match msgpack::from_slice(&frame) {
            Ok(r) => r,
            Err(e) => {
                let resp = RunResponse {
                    id: 0,
                    ok: false,
                    value: Vec::new(),
                    error: Some(format!("malformed RunRequest: {e}")),
                };
                let bytes = msgpack::to_vec_named(&resp)
                    .expect("RunResponse always serializes");
                write_frame(&mut stdout_lock, &bytes)?;
                continue;
            }
        };
        let stub_value = msgpack::to_vec_named(&"stub")
            .expect("stub value serializes");
        let resp = RunResponse {
            id: req.id,
            ok: true,
            value: stub_value,
            error: None,
        };
        let bytes = msgpack::to_vec_named(&resp)
            .expect("RunResponse always serializes");
        write_frame(&mut stdout_lock, &bytes)?;
    }
}
