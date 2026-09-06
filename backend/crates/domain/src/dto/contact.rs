use serde::Deserialize;
use validator::Validate;

use crate::types::ContactCategory;

#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ContactRequest {
    #[validate(length(min = 1, max = 100))]
    pub name: String,

    #[validate(email)]
    pub email: String,

    pub category: ContactCategory,

    #[validate(length(min = 10, max = 5000))]
    pub message: String,

    pub turnstile_token: String,

    #[serde(default = "default_locale")]
    pub locale: String,
}

fn default_locale() -> String {
    "en".to_owned()
}
