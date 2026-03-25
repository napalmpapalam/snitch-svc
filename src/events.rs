use serenity::model::id::ChannelId;

pub struct ChannelUpdate {
    pub username: String,
    pub display_name: String,
    pub channel_name: String,
    pub channel_id: ChannelId,
}

pub enum VoiceEvent {
    Joined(ChannelUpdate),
    Left(ChannelUpdate),
    InitialState(ChannelUpdate),
}

impl VoiceEvent {
    pub fn update(&self) -> &ChannelUpdate {
        match self {
            Self::Joined(u) | Self::Left(u) | Self::InitialState(u) => u,
        }
    }

    pub fn action(&self) -> &'static str {
        match self {
            Self::Joined(_) => "joined",
            Self::Left(_) => "left",
            Self::InitialState(_) => "initial_state",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            Self::Joined(_) | Self::InitialState(_) => "🎉",
            Self::Left(_) => "👋",
        }
    }

    pub fn verb(&self) -> &'static str {
        match self {
            Self::Joined(_) | Self::InitialState(_) => "joined",
            Self::Left(_) => "left",
        }
    }

    pub fn is_initial_state(&self) -> bool {
        matches!(self, Self::InitialState(_))
    }
}
