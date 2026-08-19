use crate::*;

pub struct EngineSystem {
    inner: green::InnerSystem<Self>,
    params: EngineSysParams,
}

impl green::System for EngineSystem {
    const CHANNEL_SIZE: usize = 100;
    
    type Paths = EngineSysPaths;
    type Params = EngineSysParams;
    type Config = EngineSysConfig;
    type ToSys = ToEngineSys;
    type FromSys = FromEngineSys;
    type Flow = green::StdFlow;

    fn inner(&self) -> &green::InnerSystem<Self> { &self.inner }
    fn inner_mut(&mut self) -> &mut green::InnerSystem<Self> { &mut self.inner }
    
    async fn init(inner: green::InnerSystem<Self>, params: Self::Params) -> green::GreenResult<Self> {
        Ok(Self {
            inner,
            params,
        })
    }
    
    async fn run(mut self) -> green::UnitResult {
        let result = loop {
            let result = tokio::select! {
                rx = self.inner.channel.rx.recv() => match rx {
                    Some(msg) => self.handle_channel_recv(msg).await,
                    None => Err(green::Failure),
                },
            };
            
            if let Err(e) = result {
                break Err(e);
            }
        };
        
        self.done(result).await
    }
    
    async fn on_channel_recv(&mut self, pkt: green::Packet<ToEngineSys>) -> green::FlowResult<Self::Flow> {
        let (id, nature, msg) = pkt.into_tuple();
        match (nature, msg) {
            /*(green::PacketNature::Request, ToEngineSys::MathRequest(req))
                => self.on_math_request(green::Packet::new(id, nature, req)).await,
            */
            _ => todo!(),
        }
    }

    async fn on_stop(&mut self, _halt: bool) -> green::UnitResult {
        green::Succeed
    }

    async fn on_resume(&mut self) -> green::SysResult<bool> {
        Ok(true)
    }
}

impl EngineSystem {
    pub const fn params(&self) -> &EngineSysParams { &self.params }
}