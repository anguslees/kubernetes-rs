// Implements #[serde(with="serde_base64")]

use base64;
use serde::{de, Deserialize, Deserializer, Serializer};

pub fn serialize<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.collect_str(&base64::display::Base64Display::standard(bytes))
}

pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    base64::decode(&s).map_err(de::Error::custom)
}

#[cfg(test)]
mod tests {
    use crate::serde_base64;
    use serde::{Deserialize, Serialize};
    use serde_json::{self, json};

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct Test {
        a: Vec<u8>,
        #[serde(with = "serde_base64")]
        b: Vec<u8>,
    }

    #[test]
    fn base64() {
        let input = Test {
            a: vec![123, 124],
            b: vec![126, 127],
        };
        let expected = json!({
            "a": [123, 124],
            "b": "fn8="
        });
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(json, expected);

        let roundtrip: Test = serde_json::from_value(json).unwrap();
        assert_eq!(roundtrip, input);
    }
}
