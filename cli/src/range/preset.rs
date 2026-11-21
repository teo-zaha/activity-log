use strum::{Display, EnumIter, EnumString, IntoStaticStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Display, EnumString, EnumIter, IntoStaticStr)]
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
