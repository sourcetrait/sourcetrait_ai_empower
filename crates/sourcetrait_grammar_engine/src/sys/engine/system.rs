use crate::*;

pub struct EngineSystem {
    inner: subsys::InnerSystem<Self>,
    params: EngineSysParams,
}

impl subsys::System for EngineSystem {
    const CHANNEL_SIZE: usize = 100;
    
    type Paths = EngineSysPaths;
    type Params = EngineSysParams;
    type Config = EngineSysConfig;
    type ToSys = ToEngineSys;
    type FromSys = FromEngineSys;
    type Flow = subsys::StdFlow;

    fn inner(&self) -> &subsys::InnerSystem<Self> { &self.inner }
    fn inner_mut(&mut self) -> &mut subsys::InnerSystem<Self> { &mut self.inner }
    
    async fn init(inner: subsys::InnerSystem<Self>, params: Self::Params) -> subsys::SubsysResult<Self> {
        Ok(Self {
            inner,
            params,
        })
    }
    
    async fn run(mut self) -> subsys::RunResult {
        let result = loop {
            let result = tokio::select! {
                rx = self.inner.channel.rx.recv() => match rx {
                    Some(msg) => self.handle_channel_recv(msg).await,
                    None => Err(subsys::Failure),
                },
            };
            
            if let Err(e) = result {
                break Err(e);
            }
        };
        
        self.done(result).await
    }
    
    async fn on_channel_recv(&mut self, pkt: subsys::Packet<ToEngineSys>) -> subsys::FlowResult<Self::Flow> {
        let (id, nature, msg) = pkt.into_tuple();
        match (nature, msg) {
            (subsys::PacketNature::Request, ToEngineSys::NuRepl(req))
                => self.on_nu_repl_request(subsys::Packet::new(id, nature, req)).await,
            _ => todo!(),
        }
    }

    async fn on_stop(&mut self, _halt: bool) -> subsys::RunResult {
        subsys::SUCCESS
    }

    async fn on_resume(&mut self) -> subsys::SysResult<bool> {
        Ok(true)
    }
}

impl EngineSystem {
    pub(crate) const fn params(&self) -> &EngineSysParams { &self.params }

    async fn on_nu_repl_request(&mut self, req: subsys::Packet<NuReplRequest>) -> subsys::FlowResult<subsys::StdFlow> {
        let mut bed = NubedReplBuilder.build();
        let result = bed.evaluate(&req.msg.nu).unwrap();
        
        //temp: loopback
        dbg!(&req);
        self.send_channel_packet(req.respond(FromEngineSys::NuRepl(Ok(NuReplResponse {
            result,
        })))).await?;

        subsys::CONTINUE
    }
}