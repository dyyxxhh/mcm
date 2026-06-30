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
            SourceCommand::Add { url, name, yes } => self.source_add(&url, name, yes),
            SourceCommand::Remove { identifier } => self.source_remove(&identifier),
            SourceCommand::Info { identifier } => self.source_info(&identifier),
            SourceCommand::List => self.source_list(),
        }
    }

    fn source_add(&self, url: &str, name: Option<String>, yes: bool) -> Result<()> {
        require_confirmation(OperationKind::SourceAction, yes)?;
        let mut config = self.load_config()?;
        if config.sources.contains_key(url) {
            bail!("{}", i18n::source_already_imported(self.lang, url));
        }
        // Use explicit name if provided, otherwise auto-generate "Source N"
        // where N is the next sequential number based on existing sources.
        let name = name.unwrap_or_else(|| next_source_name(&config.sources));
        let record = SourceRecord {
            url: url.to_owned(),
            name: name.clone(),
            added_at: OffsetDateTime::now_utc().to_string(),
        };
        config.sources.insert(url.to_owned(), record);
        self.save_config(&config)?;
        println!("{}", i18n::added_source(self.lang, &name));
        Ok(())
    }

    fn source_remove(&self, identifier: &str) -> Result<()> {
        let mut config = self.load_config()?;
        let key = resolve_source_key(&config.sources, identifier)
            .with_context(|| i18n::unknown_source(self.lang, identifier))?;
        let name = config.sources[&key].name.clone();
        config.sources.remove(&key);
        self.save_config(&config)?;
        println!("{}", i18n::removed_source(self.lang, &name));
        Ok(())
    }

    fn source_info(&self, identifier: &str) -> Result<()> {
        let config = self.load_config()?;
        let key = resolve_source_key(&config.sources, identifier)
            .with_context(|| i18n::unknown_source(self.lang, identifier))?;
        let record = &config.sources[&key];
        println!("name: {}", record.name);
        println!("url: {}", record.url);
        println!("status: {}", i18n::source_status_trusted(self.lang));
        println!("added_at: {}", record.added_at);
        if record.url.starts_with("http") {
            match crate::source_index::fetch_source_index(&record.url) {
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
        for record in config.sources.values() {
            println!("{}  {}", record.name, record.url);
        }
        Ok(())
    }
}

/// Auto-generate the next sequential source name: "Source 1", "Source 2", …
/// Counts how many existing names already match the "Source N" pattern and
/// picks the next unused number.
fn next_source_name(sources: &std::collections::BTreeMap<String, SourceRecord>) -> String {
    let used: std::collections::HashSet<u32> = sources
        .values()
        .filter_map(|r| {
            r.name
                .strip_prefix("Source ")
                .and_then(|n| n.parse::<u32>().ok())
        })
        .collect();
    (1..)
        .find(|n| !used.contains(n))
        .map(|n| format!("Source {n}"))
        .expect("source name counter cannot overflow")
}

/// Look up a source by URL or by name. Returns the map key (URL) on success.
fn resolve_source_key(
    sources: &std::collections::BTreeMap<String, SourceRecord>,
    identifier: &str,
) -> Option<String> {
    // Direct URL match.
    if sources.contains_key(identifier) {
        return Some(identifier.to_owned());
    }
    // Name match (case-sensitive, first hit).
    sources
        .iter()
        .find(|(_, r)| r.name == identifier)
        .map(|(k, _)| k.clone())
}
