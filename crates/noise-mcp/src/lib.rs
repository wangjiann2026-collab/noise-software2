//! # noise-mcp
//!
//! MCP (Model Context Protocol) server that exposes the acoustic computation
//! platform to AI agents (e.g., Claude via claude-code or API).
//!
//! ## Exposed MCP Tools
//! | Tool name                    | Description |
//! |------------------------------|-------------|
//! | `noise_calculate`            | Run a noise calculation on a scenario |
//! | `noise_query_grid`           | Query horizontal grid results |
//! | `noise_query_building_facade`| Query facade noise results |
//! | `noise_list_scenarios`       | List project scenarios and variants |
//! | `noise_add_source`           | Add a noise source to a scenario |
//! | `noise_add_building`         | Add a building to a scenario |
//! | `noise_get_metrics`          | Get Ld/Ln/Lden/custom at a point |

pub mod schema;
pub mod server;
pub mod tools;
