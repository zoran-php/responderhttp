// http_client/src-tauri/src/persistence/repositories/import.rs
//
// Writes an OpenAPI import in one transaction (PLAN.md Phase 8d, step 6).
// Every row goes through the same insert function its own repository uses,
// so an imported request is stored — and its auth sealed — exactly like a
// saved one. If anything fails, nothing is kept.
use std::sync::Arc;

use crate::domain::error::AppError;
use crate::domain::ids::new_id;
use crate::domain::import_plan::{ImportPlan, ImportedIds};
use crate::domain::models::{Collection, Environment, Folder, NewExample, SavedRequest};
use crate::domain::ports::{ImportRepository, SecretCipher};
use crate::domain::secrets::SecretState;
use crate::persistence::database::{to_storage_error, Database};
use crate::persistence::repositories::collections::insert_collection;
use crate::persistence::repositories::environments::{
    insert_environment, insert_variables, prepare_variables,
};
use crate::persistence::repositories::examples::insert_example;
use crate::persistence::repositories::folders::insert_folder;
use crate::persistence::repositories::saved_requests::{now_iso8601, upsert_request};

pub struct SqliteImportRepository {
    database: Database,
    cipher: Arc<dyn SecretCipher>,
}

impl SqliteImportRepository {
    pub fn new(database: Database, cipher: Arc<dyn SecretCipher>) -> Self {
        Self { database, cipher }
    }
}

impl ImportRepository for SqliteImportRepository {
    fn import(&self, plan: &ImportPlan) -> Result<ImportedIds, AppError> {
        let now = now_iso8601();
        let collection = Collection {
            id: new_id("col"),
            name: plan.collection_name.trim().to_string(),
        };

        let mut folder_ids: Vec<String> = Vec::with_capacity(plan.folders.len());
        let mut folders = Vec::with_capacity(plan.folders.len());
        for (index, planned) in plan.folders.iter().enumerate() {
            let parent_folder_id = match planned.parent {
                Some(parent) if parent < index => Some(folder_ids[parent].clone()),
                Some(parent) => {
                    return Err(AppError::Internal(format!(
                        "import plan lists folder {index} before its parent {parent}"
                    )))
                }
                None => None,
            };
            let id = new_id("fld");
            folder_ids.push(id.clone());
            folders.push(Folder {
                id,
                collection_id: collection.id.clone(),
                parent_folder_id,
                name: planned.name.trim().to_string(),
            });
        }

        // Environment secrets are sealed before the lock is taken, as the
        // environment repository does.
        let environment = match &plan.environment {
            Some(planned) => {
                let environment = Environment {
                    id: new_id("env"),
                    name: planned.name.trim().to_string(),
                };
                let prepared =
                    prepare_variables(self.cipher.as_ref(), &environment.id, &planned.variables)?;
                Some((environment, prepared))
            }
            None => None,
        };

        let mut guard = self.database.lock();
        let transaction = guard.transaction().map_err(to_storage_error)?;

        insert_collection(&transaction, &collection, &now)?;
        for folder in &folders {
            insert_folder(&transaction, folder, &now)?;
        }
        for planned in &plan.requests {
            let folder_id = match planned.folder {
                Some(index) => Some(folder_ids.get(index).cloned().ok_or_else(|| {
                    AppError::Internal(format!("import plan names missing folder {index}"))
                })?),
                None => None,
            };
            let saved = SavedRequest {
                id: new_id("req"),
                collection_id: collection.id.clone(),
                folder_id,
                name: planned.name.trim().to_string(),
                request: planned.request.clone(),
                secret_state: SecretState::Ok,
            };
            upsert_request(&transaction, &saved, self.cipher.as_ref(), &now)?;
            for example in &planned.examples {
                let new_example = NewExample {
                    request_id: saved.id.clone(),
                    name: example.name.clone(),
                    request: planned.request.clone(),
                    status: example.status,
                    response_headers: example.response_headers.clone(),
                    response_body: example.response_body.clone(),
                };
                insert_example(&transaction, &new_id("exa"), &now, &new_example)?;
            }
        }
        if let Some((environment, prepared)) = &environment {
            insert_environment(&transaction, environment, &now)?;
            insert_variables(&transaction, &environment.id, prepared)?;
        }
        transaction.commit().map_err(to_storage_error)?;

        Ok(ImportedIds {
            collection_id: collection.id,
            environment_id: environment.map(|(environment, _)| environment.id),
        })
    }
}
