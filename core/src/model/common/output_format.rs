use strum::{AsRefStr, Display, EnumString};

#[derive(Debug, Clone, PartialEq, Eq, Display, AsRefStr, EnumString)]
#[strum(serialize_all = "lowercase", ascii_case_insensitive)]
pub enum OutputFormat {
    Json,
    Markdown,
}
