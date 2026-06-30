//! `source` command group implementations on [`App`].
//!
//! Manages manually imported custom sources. Importing a source makes it
//! trusted; the actionable `add` operation still requires confirmation via
//! the centralized policy (`require_confirmation(OperationKind::SourceAction)`).
//! Fresh config has zero custom sources — no author source is preinstalled.

use anyhow::{bail, Context, Result};
use time::OffsetDateTime;

use crate::app::App;
use crate::cli::SourceCommand;
use crate::config::SourceRecord;
use crate::confirmation::{require_confirmation, OperationKind};
use crate::i18n;

impl App {
    pub(crate) fn source(&self, command: SourceCommand) -> Result<()> {
        match command {
            SourceCommand::Add { url, yes } => self.source_add(&url, yes),
            SourceCommand::Remove { url } => self.source_remove(&url),
            SourceCommand::Info { url } => self.source_info(&url),
            SourceCommand::List => self.source_list(),
        }
    }

    fn source_add(&self, url: &str, yes: bool) -> Result<()> {
        require_confirmation(OperationKind::SourceAction, yes)?;
        let mut config = self.load_config()?;
        if config.sources.contains_key(url) {
            bail!("{}", i18n::source_already_imported(self.lang, url));
        }
        let record = SourceRecord {
            url: url.to_owned(),
            added_at: OffsetDateTime::now_utc().to_string(),
        };
        config.sources.insert(url.to_owned(), record);
        self.save_config(&config)?;
        println!("{}", i18n::added_source(self.lang, url));
        Ok(())
    }

    fn source_remove(&self, url: &str) -> Result<()> {
        let mut config = self.load_config()?;
        config
            .sources
            .remove(url)
            .with_context(|| i18n::unknown_source(self.lang, url))?;
        self.save_config(&config)?;
        println!("{}", i18n::removed_source(self.lang, url));
        Ok(())
    }

    fn source_info(&self, url: &str) -> Result<()> {
        let config = self.load_config()?;
        let record = config
            .sources
            .get(url)
            .with_context(|| i18n::unknown_source(self.lang, url))?;
        println!("url: {}", record.url);
        println!("status: {}", i18n::source_status_trusted(self.lang));
        println!("added_at: {}", record.added_at);
        if url.starts_with("http") {
            match crate::source_index::fetch_source_index(url) {
                Ok(index) => {
                    println!("source_id: {}", index.source_id);
                    if !index.capabilities.is_empty() {
                        println!("capabilities: {}", index.capabilities.join(", "));
                    }
                    println!("packages: {}", index.packages.len());
                    if let Some(actions) = &index.actions {
                        println!("actions: {} (declared, not auto-executed)", actions.len());
                    }
                }
                Err(error) => {
                    println!(
                        "{}",
                        i18n::source_index_unavailable(self.lang, &error.to_string())
                    );
                }
            }
        }
        Ok(())
    }

    fn source_list(&self) -> Result<()> {
        let config = self.load_config()?;
        for url in config.sources.keys() {
            println!("{url}");
        }
        Ok(())
    }
}
