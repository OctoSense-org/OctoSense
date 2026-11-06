//! Events a native host (the Android or OpenHarmony shell around this app) posts as actions:
//! the floating composer's text and buttons, a deep link, and a system-browser document's
//! navigation. The fork makepad defined these in its platform; they are the host's protocol
//! with this app, so they live here. A host that posts none leaves every downcast unmatched.

/// The native floating composer submitted text.
#[derive(Clone, Debug, Default)]
pub struct NativeComposerSubmit {
    pub text: String,
}

/// A system-browser document navigated. Carries the browser it came from.
#[derive(Clone, Debug, Default)]
pub struct NativeSystemBrowserNavigation {
    pub browser_id: u64,
    pub url: String,
    pub title: String,
    pub loading: bool,
    pub error: Option<String>,
}

/// The app was opened through a deep link.
#[derive(Clone, Debug, Default)]
pub struct NativeDeepLink {
    pub url: String,
}

/// The composer's "+" (open another app) button was tapped. A bare action.
#[derive(Clone, Debug, Default)]
pub struct NativeComposerNewApp;

/// The composer's switch-to-next-app button was tapped. A bare action.
#[derive(Clone, Debug, Default)]
pub struct NativeComposerSwitch;

/// The composer's collapsed "+" FAB was tapped to unfold it.
#[derive(Clone, Debug, Default)]
pub struct NativeComposerExpand;
