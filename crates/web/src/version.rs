/// Build provenance baked in at compile time by `build.rs`.
pub struct BuildInfo;

impl BuildInfo {
    /// `git describe --tags --always --dirty` — e.g. `v0.1.0-5-g162ee50`,
    /// or a bare short SHA before any tags exist.
    pub const VERSION: &'static str = env!("DUCK_BUILD_VERSION");
    /// Short commit SHA.
    pub const SHA: &'static str = env!("DUCK_GIT_SHA");

    /// The SHA, unless the version string already displays it (which
    /// `git describe --always` does before any tags exist).
    pub fn distinct_sha() -> Option<&'static str> {
        let stripped = Self::SHA;
        (!Self::VERSION.contains(stripped.get(..7).unwrap_or(stripped)))
            .then_some(stripped)
    }
}
