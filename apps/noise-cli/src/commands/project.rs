use clap::{Args, Subcommand};

#[derive(Args)]
pub struct ProjectArgs {
    #[command(subcommand)]
    pub action: ProjectAction,
}

#[derive(Subcommand)]
pub enum ProjectAction {
    /// Create a new project.
    New {
        /// Project name.
        #[arg(short, long)]
        name: String,
        /// EPSG coordinate reference system code.
        #[arg(long, default_value = "32650")]
        crs: u32,
        /// Output project file path.
        #[arg(short, long, default_value = "project.nsp")]
        file: String,
    },
    /// List scenarios in a project.
    List {
        #[arg(short, long)]
        project: String,
    },
    /// Add a scenario variant.
    AddVariant {
        #[arg(short, long)]
        project: String,
        #[arg(short, long)]
        name: String,
        #[arg(long)]
        notes: Option<String>,
    },
}

pub async fn run(args: ProjectArgs) -> anyhow::Result<()> {
    match args.action {
        ProjectAction::New { name, crs, file } => {
            use noise_data::scenario::Project;
            let project = Project::new(&name, crs);
            let json = serde_json::to_string_pretty(&project)?;
            std::fs::write(&file, json)?;
            println!("Project '{}' created → {}", name, file);
        }
        ProjectAction::List { project } => {
            let json = std::fs::read_to_string(&project)?;
            let p: noise_data::scenario::Project = serde_json::from_str(&json)?;
            println!("Project: {}", p.name);
            println!("  Base: {}", p.base_scenario.name);
            for (i, v) in p.variants.iter().enumerate() {
                println!("  Variant {}: {}", i + 1, v.name);
            }
        }
        ProjectAction::AddVariant { project, name, notes } => {
            let json = std::fs::read_to_string(&project)?;
            let mut p: noise_data::scenario::Project = serde_json::from_str(&json)?;
            {
                let v = p.add_variant(&name);
                println!("Variant '{}' added (ID: {})", v.name, v.id);
            }
            if let Some(note) = notes {
                if let Some(v) = p.variants.last_mut() {
                    v.strategy_notes = note;
                }
            }
            std::fs::write(&project, serde_json::to_string_pretty(&p)?)?;
        }
    }
    Ok(())
}
