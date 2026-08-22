use crate::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineParameters {
    pub equip_id: Option<String>,
    pub equip_namespace: Option<String>,
}

impl EngineParameters {
    pub const DEFAULT: Self = Self{
        equip_id: None,
        equip_namespace: None,
    };
    
    pub const fn new() -> Self { Self::DEFAULT }
}

#[cereal::derived(Data, Copy, Eq)]
#[repr(u8)]
pub enum EngineParameterKind {
    EquipId,
    EquipNamespace,
}

impl Default for EngineParameters { fn default() -> Self { Self::DEFAULT } }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineBuilder {
    pub parameters: EngineParameters,
}

impl EngineBuilder {
    pub const DEFAULT: Self = Self {
        parameters: EngineParameters::DEFAULT,
    };
    
    pub const fn new() -> Self { Self::DEFAULT }

    pub fn equip_id<S: Into<String>>(mut self, value: S) -> Self {
        self.parameters.equip_id = Some(value.into());
        self
    }
    
    pub fn equip_namespace<S: Into<String>>(mut self, value: S) -> Self {
        self.parameters.equip_namespace = Some(value.into());
        self
    }

    pub async fn start(self) -> GrammarEngineResult<Engine> {
        let params = EngineSysParams::try_from_face(self.parameters)?;
        let paths = EngineSysPaths::default();
        let config = EngineSysConfig::default();
        let control: green::SystemControl<EngineSystem> = green::SystemControl::start(
            paths,
            config,
            params,
        ).await.unwrap();

        Ok(Engine {
            control,
        })
    }
}

impl Default for EngineBuilder { fn default() -> Self { Self::DEFAULT } }

pub struct Engine {
    control: green::SystemControl<EngineSystem>,
}

impl Engine {
    pub async fn request<T: EngineRequest>(&self, req: T) -> GrammarEngineResult<T::ResponseType> {
        let packet = green::Packet::request(req.into());
        self.control.send_packet(packet).await
            .unwrap(); //todo

        todo!()
    }
}

