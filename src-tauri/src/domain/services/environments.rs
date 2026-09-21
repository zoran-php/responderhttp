// http_client/src-tauri/src/domain/services/environments.rs
//
// Use-cases over the environment aggregate. Variables belong to an
// environment, so they live behind the same repository rather than getting a
// second trait (CLAUDE.md section 7, one trait per aggregate).
use std::sync::Arc;

use crate::domain::error::AppError;
use crate::domain::models::{Environment, EnvironmentVariable};
use crate::domain::ports::EnvironmentRepository;
use crate::domain::services::validation::validated_name;

/// Cheap to clone (Arc), so the command layer can move a clone into a
/// blocking task.
#[derive(Clone)]
pub struct Environments {
    environments: Arc<dyn EnvironmentRepository>,
}

impl Environments {
    pub fn new(environments: Arc<dyn EnvironmentRepository>) -> Self {
        Self { environments }
    }

    pub fn list(&self) -> Result<Vec<Environment>, AppError> {
        self.environments.list()
    }

    pub fn create(&self, name: &str) -> Result<Environment, AppError> {
        self.environments.create(validated_name(name)?)
    }

    pub fn rename(&self, id: &str, name: &str) -> Result<(), AppError> {
        self.environments.rename(id, validated_name(name)?)
    }

    pub fn delete(&self, id: &str) -> Result<(), AppError> {
        self.environments.delete(id)
    }

    pub fn variables(&self, environment_id: &str) -> Result<Vec<EnvironmentVariable>, AppError> {
        self.environments.variables(environment_id)
    }

    pub fn set_variables(
        &self,
        environment_id: &str,
        variables: &[EnvironmentVariable],
    ) -> Result<(), AppError> {
        self.environments.set_variables(environment_id, variables)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct SpyEnvironments {
        created: Mutex<Vec<String>>,
    }

    impl EnvironmentRepository for SpyEnvironments {
        fn list(&self) -> Result<Vec<Environment>, AppError> {
            Ok(Vec::new())
        }

        fn create(&self, name: &str) -> Result<Environment, AppError> {
            self.created
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(name.to_string());
            Ok(Environment {
                id: "env_1".into(),
                name: name.to_string(),
            })
        }

        fn rename(&self, _id: &str, _name: &str) -> Result<(), AppError> {
            Ok(())
        }

        fn delete(&self, _id: &str) -> Result<(), AppError> {
            Ok(())
        }

        fn variables(&self, _environment_id: &str) -> Result<Vec<EnvironmentVariable>, AppError> {
            Ok(Vec::new())
        }

        fn set_variables(
            &self,
            _environment_id: &str,
            _variables: &[EnvironmentVariable],
        ) -> Result<(), AppError> {
            Ok(())
        }
    }

    #[test]
    fn trims_a_name_before_storing_it() {
        let spy = Arc::new(SpyEnvironments::default());
        let service = Environments::new(spy.clone());

        service.create("  Staging  ").expect("should create");

        let created = spy
            .created
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        assert_eq!(created.as_slice(), ["Staging"]);
    }

    #[test]
    fn refuses_a_blank_name() {
        let service = Environments::new(Arc::new(SpyEnvironments::default()));

        assert!(service.create("   ").is_err());
        assert!(service.rename("env_1", "").is_err());
    }
}
