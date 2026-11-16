use strum::{Display, EnumIter, EnumString};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Display, EnumString, EnumIter)]
#[strum(serialize_all = "camelCase", ascii_case_insensitive)]
pub enum RangePreset {
    Today,
    Yesterday,
    Week,
    #[strum(serialize = "lastweek")]
    LastWeek,
    Month,
    #[strum(serialize = "lastmonth")]
    LastMonth,
}
