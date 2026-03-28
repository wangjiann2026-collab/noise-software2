//! CLI commands for managing scene objects.
//!
//! Usage examples:
//!   noise object add-receiver --project p.nsp --x 100 --y 200
//!   noise object list --project p.nsp --type receiver
//!   noise object delete --project p.nsp --db project.db --id 42

use clap::{Args, Subcommand};

#[derive(Args)]
pub struct ObjectArgs {
    #[command(subcommand)]
    pub action: ObjectAction,
}

#[derive(Subcommand)]
pub enum ObjectAction {
    /// Add a receiver point to a scenario.
    AddReceiver {
        #[arg(short, long)] project: String,
        #[arg(long)]        db: Option<String>,
        #[arg(long)]        name: Option<String>,
        #[arg(long)]        x: f64,
        #[arg(long)]        y: f64,
        #[arg(long, default_value_t = 0.0)] z: f64,
        #[arg(long, default_value_t = 4.0)] height: f64,
        #[arg(long)]        scenario: Option<String>,
    },
    /// List scene objects in a scenario.
    List {
        #[arg(short, long)] project: String,
        #[arg(long)]        db: Option<String>,
        #[arg(long)]        object_type: Option<String>,
        #[arg(long)]        scenario: Option<String>,
    },
    /// Delete an object by its database row ID.
    Delete {
        #[arg(short, long)] project: String,
        #[arg(long)]        db: Option<String>,
        #[arg(long)]        id: i64,
        #[arg(long)]        scenario: Option<String>,
    },
    /// Count objects in a scenario.
    Count {
        #[arg(short, long)] project: String,
        #[arg(long)]        db: Option<String>,
        #[arg(long)]        object_type: Option<String>,
        #[arg(long)]        scenario: Option<String>,
    },
}

pub async fn run(args: ObjectArgs) -> anyhow::Result<()> {
    use noise_data::db::Database;
    use noise_data::entities::{ObjectType, ReceiverPoint, SceneObject};
    use noise_data::repository::{ProjectRepository, SceneObjectRepository};
    use noise_data::scenario::Project;
    use nalgebra::Point3;

    match args.action {
        ObjectAction::AddReceiver { project, db, name, x, y, z, height, scenario } => {
            let db_path = db.unwrap_or_else(|| project.replace(".nsp", ".db"));
            let proj: Project = serde_json::from_str(&std::fs::read_to_string(&project)?)?;
            let database = Database::open(&db_path)?;
            let pr = ProjectRepository::new(database.connection());
            pr.insert(&proj)?;

            let sid = scenario
                .unwrap_or_else(|| proj.base_scenario.id.to_string());

            // Auto-generate sequential ID based on count.
            let repo = SceneObjectRepository::new(database.connection());
            let count = repo.count(&sid, Some(ObjectType::Receiver))?;
            let id = count + 1;
            let obj_name = name.unwrap_or_else(|| format!("Receiver_{id}"));
            let obj = SceneObject::Receiver(ReceiverPoint::new(id, &obj_name, x, y, z, height));
            let row_id = repo.insert(&sid, &obj)?;
            println!("Receiver '{obj_name}' added (row_id={row_id}, x={x}, y={y}, h={height}m)");
        }

        ObjectAction::List { project, db, object_type, scenario } => {
            let db_path = db.unwrap_or_else(|| project.replace(".nsp", ".db"));
            let proj: Project = serde_json::from_str(&std::fs::read_to_string(&project)?)?;
            let database = Database::open(&db_path)?;
            let pr = ProjectRepository::new(database.connection());
            pr.insert(&proj)?;
            let sid = scenario.unwrap_or_else(|| proj.base_scenario.id.to_string());

            let ot = object_type.as_deref().and_then(ObjectType::from_str);
            let repo = SceneObjectRepository::new(database.connection());
            let objects = repo.list(&sid, ot)?;
            if objects.is_empty() {
                println!("No objects found.");
            } else {
                println!("{:<8} {:<16} {}", "RowID", "Type", "Name");
                println!("{}", "-".repeat(50));
                for (row_id, obj) in &objects {
                    println!("{:<8} {:<16} {}", row_id, obj.object_type().as_str(), obj.name());
                }
                println!("Total: {}", objects.len());
            }
        }

        ObjectAction::Delete { project, db, id, scenario } => {
            let db_path = db.unwrap_or_else(|| project.replace(".nsp", ".db"));
            let proj: Project = serde_json::from_str(&std::fs::read_to_string(&project)?)?;
            let database = Database::open(&db_path)?;
            let pr = ProjectRepository::new(database.connection());
            pr.insert(&proj)?;
            let _ = scenario;
            let repo = SceneObjectRepository::new(database.connection());
            repo.delete(id)?;
            println!("Object row_id={id} deleted.");
        }

        ObjectAction::Count { project, db, object_type, scenario } => {
            let db_path = db.unwrap_or_else(|| project.replace(".nsp", ".db"));
            let proj: Project = serde_json::from_str(&std::fs::read_to_string(&project)?)?;
            let database = Database::open(&db_path)?;
            let pr = ProjectRepository::new(database.connection());
            pr.insert(&proj)?;
            let sid = scenario.unwrap_or_else(|| proj.base_scenario.id.to_string());
            let ot = object_type.as_deref().and_then(ObjectType::from_str);
            let repo = SceneObjectRepository::new(database.connection());
            let count = repo.count(&sid, ot)?;
            println!("Count: {count}");
        }
    }
    Ok(())
}
