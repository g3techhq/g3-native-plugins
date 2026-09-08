use serde::{Deserialize, Serialize};
/// Whether the app may use something the user has to agree to.
///
/// Shared by every plugin here that asks permission, so an app can handle a
/// refusal the same way whatever was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PermissionState {
    /// Not yet decided: the user has not been asked.
    #[default]
    Prompt,
    /// Refused once, and Android wants a reason shown before asking again.
    /// Never reported on iOS, which does not have the concept.
    PromptWithRationale,
    /// Granted.
    Granted,
    /// Refused.
    ///
    /// On both platforms asking again does nothing once it means "permanently
    /// denied", which is the state it settles into: iOS only ever prompts once,
    /// and Android stops showing the dialog after a second refusal. The only
    /// way back is the Settings app, which is what
    /// `CameraMicrophone::open_settings` is for.
    Denied,
}
