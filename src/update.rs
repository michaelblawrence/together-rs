use self_update::cargo_crate_version;

use crate::log;

pub fn update() -> Result<(), Box<dyn std::error::Error>> {
    let updater = self_update::backends::github::Update::configure()
        .repo_owner("michaelblawrence")
        .repo_name("together-rs")
        .bin_name("together-rs")
        .show_download_progress(true)
        .show_output(false)
        .rewrite_aarch64_targets()
        .current_version(cargo_crate_version!())
        .build()?;

    let status = updater.update()?;

    match status {
        self_update::Status::UpToDate(_) => (),
        self_update::Status::Updated(v) => {
            log!("Updated version of together to: {}", v);
        }
    }

    Ok(())
}

trait UpdateBuilderExt {
    fn rewrite_aarch64_targets(&mut self) -> &mut Self;
}

impl UpdateBuilderExt for self_update::backends::github::UpdateBuilder {
    fn rewrite_aarch64_targets(&mut self) -> &mut Self {
        #[cfg(all(target_arch = "aarch64", target_os = "linux"))]
        {
            self.target("x86_64-unknown-linux-gnu");
        }

        #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
        {
            self.target("x86_64-apple-darwin");
        }

        self
    }
}
