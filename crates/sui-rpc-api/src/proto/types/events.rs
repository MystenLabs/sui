use super::TryFromProtoError;

//
// Event
//

impl From<sui_sdk_types::Event> for super::Event {
    fn from(value: sui_sdk_types::Event) -> Self {
        Self {
            package_id: Some(value.package_id.into()),
            module: Some(value.module.into()),
            sender: Some(value.sender.into()),
            event_type: Some(value.type_.into()),
            contents: Some(value.contents.into()),
        }
    }
}

impl TryFrom<&super::Event> for sui_sdk_types::Event {
    type Error = TryFromProtoError;

    fn try_from(value: &super::Event) -> Result<Self, Self::Error> {
        let package_id = value
            .package_id
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("package_id"))?
            .try_into()?;

        let module = value
            .module
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("module"))?
            .try_into()?;

        let sender = value
            .sender
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("sender"))?
            .try_into()?;

        let type_ = value
            .event_type
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("event_type"))?
            .try_into()?;

        let contents = value
            .contents
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("contents"))?
            .to_vec();

        Ok(Self {
            package_id,
            module,
            sender,
            type_,
            contents,
        })
    }
}

//
// TransactionEvents
//

impl From<sui_sdk_types::TransactionEvents> for super::TransactionEvents {
    fn from(value: sui_sdk_types::TransactionEvents) -> Self {
        Self {
            events: value.0.into_iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<&super::TransactionEvents> for sui_sdk_types::TransactionEvents {
    type Error = TryFromProtoError;

    fn try_from(value: &super::TransactionEvents) -> Result<Self, Self::Error> {
        Ok(Self(
            value
                .events
                .iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        ))
    }
}
