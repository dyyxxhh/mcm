use anyhow::{bail, Context, Result};

use crate::config::Profile;
use crate::provider::{group_projects, Artifact, Project, Provider};

pub(crate) struct CompositeProvider {
    providers: Vec<Box<dyn Provider>>,
}

impl CompositeProvider {
    pub(crate) fn new(providers: Vec<Box<dyn Provider>>) -> Self {
        Self { providers }
    }

    pub(crate) fn default() -> Result<Self> {
        let mut providers: Vec<Box<dyn Provider>> = vec![Box::new(super::ModrinthProvider::new())];
        match super::CurseForgeProvider::new() {
            Ok(provider) => providers.push(Box::new(provider)),
            Err(error) => eprintln!("warning: CurseForge disabled: {error}"),
        }
        Ok(Self::new(providers))
    }
}

impl Provider for CompositeProvider {
    fn search(&self, query: &str, profile: &Profile) -> Result<Vec<Project>> {
        let mut projects = Vec::new();
        let mut warnings = Vec::new();
        for provider in &self.providers {
            match provider.search(query, profile) {
                Ok(mut found) => projects.append(&mut found),
                Err(error) => warnings.push(error.to_string()),
            }
        }
        for warning in warnings {
            eprintln!("warning: provider search failed: {warning}");
        }
        Ok(group_projects(projects))
    }

    fn get(&self, query: &str, profile: &Profile) -> Result<Project> {
        let mut projects = Vec::new();
        let mut errors = Vec::new();
        for provider in &self.providers {
            match provider.get(query, profile) {
                Ok(project) => projects.push(project),
                Err(error) => errors.push(error.to_string()),
            }
        }
        let mut grouped = group_projects(projects);
        grouped
            .pop()
            .with_context(|| format!("mod {query} not found ({})", errors.join("; ")))
    }

    fn download(&self, artifact: &Artifact) -> Result<Vec<u8>> {
        let mut errors = Vec::new();
        for provider in &self.providers {
            match provider.download(artifact) {
                Ok(bytes) => return Ok(bytes),
                Err(error) => errors.push(error.to_string()),
            }
        }
        bail!("download failed: {}", errors.join("; "))
    }
}
