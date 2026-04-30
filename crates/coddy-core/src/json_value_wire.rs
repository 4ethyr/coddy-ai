use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub(crate) fn serialize<S>(value: &serde_json::Value, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    if serializer.is_human_readable() {
        value.serialize(serializer)
    } else {
        serde_json::to_string(value)
            .map_err(serde::ser::Error::custom)?
            .serialize(serializer)
    }
}

pub(crate) fn deserialize<'de, D>(deserializer: D) -> Result<serde_json::Value, D::Error>
where
    D: Deserializer<'de>,
{
    if deserializer.is_human_readable() {
        serde_json::Value::deserialize(deserializer)
    } else {
        let json = String::deserialize(deserializer)?;
        serde_json::from_str(&json).map_err(serde::de::Error::custom)
    }
}
