use crate::config::ALLOWED_EXTENSIONS;
use crate::extend_types::ExtendType;
use crate::state::{Data, Entity, GraphQL, GraphQLType, Node};

use anyhow::Result;
use async_std::{
    fs,
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    prelude::*,
    sync::{Arc, Mutex},
};
use graphql_parser::{parse_schema, schema};
use petgraph::graph::NodeIndex;
use std::collections::HashMap;

fn is_extension_allowed(extension: &str) -> bool {
    ALLOWED_EXTENSIONS.to_vec().contains(&extension)
}

fn get_extended_id(id: String) -> String {
    format!("{}Ext", id)
}

/// Recursively read directories and files for a given path.
pub fn get_files(
    path: PathBuf,
    shared_data: Arc<Mutex<Data>>,
) -> Pin<Box<dyn Future<Output = Result<()>>>> {
    // Use a hack to get async recursive calls working.
    Box::pin(async move {
        let thread_safe_path = Arc::new(path);
        let file_or_dir = fs::metadata(thread_safe_path.as_ref()).await?;
        let file_type = file_or_dir.file_type();

        if file_type.is_file() {
            if is_extension_allowed(
                Path::new(thread_safe_path.as_ref())
                    .extension()
                    .unwrap()
                    .to_str()
                    .unwrap(),
            ) {
                let contents = fs::read_to_string(thread_safe_path.as_ref()).await?;
                let mut data = shared_data.lock().await;

                data.files
                    .insert(thread_safe_path.as_ref().clone(), contents);
            }

            return Ok(());
        }

        let mut dir = fs::read_dir(thread_safe_path.as_ref()).await?;

        while let Some(result) = dir.next().await {
            let entry: fs::DirEntry = result?;
            let inner_path = entry.path();
            let inner_path_cloned = inner_path.clone();
            let metadata = entry.clone().metadata().await?;
            let is_dir = metadata.is_dir();

            if !is_dir && is_extension_allowed(&inner_path.extension().unwrap().to_str().unwrap()) {
                let contents = fs::read_to_string(inner_path).await?;
                let mut data = shared_data.lock().await;

                data.files.insert(inner_path_cloned, contents);
            } else {
                get_files(inner_path, shared_data.clone()).await?;
            }
        }

        Ok(())
    })
}

/// Parse the files, generate an AST and walk it to populate the graph.
pub async fn populate_graph_from_ast(shared_data: Arc<Mutex<Data>>) -> Result<()> {
    let mut data = shared_data.lock().await;
    let files = &data.files.clone();
    // Keep track of the dependencies for edges.
    let mut dependency_hash_map: HashMap<NodeIndex, Vec<String>> = HashMap::new();

    // Populate the nodes first.
    for (file, contents) in files {
        let ast = parse_schema::<String>(contents.as_str())?;

        // Reference: http://spec.graphql.org/draft/
        for definition in ast.definitions {
            match definition {
                schema::Definition::TypeDefinition(type_definition) => match type_definition {
                    schema::TypeDefinition::Enum(enum_type) => {
                        let id = enum_type.name.clone();
                        let dependencies = enum_type.get_dependencies();

                        let node_index = data.graph.add_node(Node::new(
                            Entity::new(
                                dependencies.clone(), // Enums don't have dependencies.
                                GraphQL::TypeDefinition(GraphQLType::Enum),
                                enum_type.name,
                                file.to_owned(),
                                contents.to_owned(),
                            ),
                            id,
                        ));

                        // Update dependencies.
                        dependency_hash_map.insert(node_index, dependencies);
                    }

                    schema::TypeDefinition::InputObject(input_object_type) => {
                        let id = input_object_type.name.clone();
                        let dependencies = input_object_type.get_dependencies();

                        let node_index = data.graph.add_node(Node::new(
                            Entity::new(
                                dependencies.clone(),
                                GraphQL::TypeDefinition(GraphQLType::InputObject),
                                input_object_type.name,
                                file.to_owned(),
                                contents.to_owned(),
                            ),
                            id,
                        ));

                        dependency_hash_map.insert(node_index, dependencies);
                    }

                    schema::TypeDefinition::Interface(interface_type) => {
                        let id = interface_type.name.clone();
                        let dependencies = interface_type.get_dependencies();

                        let node_index = data.graph.add_node(Node::new(
                            Entity::new(
                                dependencies.clone(),
                                GraphQL::TypeDefinition(GraphQLType::Interface),
                                interface_type.name,
                                file.to_owned(),
                                contents.to_owned(),
                            ),
                            id,
                        ));

                        dependency_hash_map.insert(node_index, dependencies);
                    }

                    schema::TypeDefinition::Object(object_type) => {
                        let id = object_type.name.clone();
                        let dependencies = object_type.get_dependencies();

                        let node_index = data.graph.add_node(Node::new(
                            Entity::new(
                                dependencies.clone(),
                                GraphQL::TypeDefinition(GraphQLType::Object),
                                object_type.name,
                                file.to_owned(),
                                contents.to_owned(),
                            ),
                            id,
                        ));

                        dependency_hash_map.insert(node_index, dependencies);
                    }

                    schema::TypeDefinition::Scalar(scalar_type) => {
                        let id = scalar_type.name.clone();
                        let dependencies = scalar_type.get_dependencies();

                        let node_index = data.graph.add_node(Node::new(
                            Entity::new(
                                dependencies.clone(),
                                GraphQL::TypeDefinition(GraphQLType::Scalar),
                                scalar_type.name,
                                file.to_owned(),
                                contents.to_owned(),
                            ),
                            id,
                        ));

                        dependency_hash_map.insert(node_index, dependencies);
                    }

                    schema::TypeDefinition::Union(union_type) => {
                        let id = union_type.name.clone();
                        let dependencies = union_type.get_dependencies();

                        let node_index = data.graph.add_node(Node::new(
                            Entity::new(
                                dependencies.clone(),
                                GraphQL::TypeDefinition(GraphQLType::Union),
                                union_type.name,
                                file.to_owned(),
                                contents.to_owned(),
                            ),
                            id,
                        ));

                        dependency_hash_map.insert(node_index, dependencies);
                    }
                },

                schema::Definition::SchemaDefinition(schema_definition) => {
                    // A Schema has no name, use a default one.
                    let id = String::from("Schema");
                    let dependencies = schema_definition.get_dependencies();

                    let node_index = data.graph.add_node(Node::new(
                        Entity::new(
                            dependencies.clone(),
                            GraphQL::Schema,
                            String::from("Schema"),
                            file.to_owned(),
                            contents.to_owned(),
                        ),
                        id,
                    ));

                    dependency_hash_map.insert(node_index, dependencies);
                }

                schema::Definition::DirectiveDefinition(directive_definition) => {
                    let id = directive_definition.name.clone();
                    let dependencies = directive_definition.get_dependencies();

                    let node_index = data.graph.add_node(Node::new(
                        Entity::new(
                            dependencies.clone(),
                            GraphQL::Directive,
                            directive_definition.name,
                            file.to_owned(),
                            contents.to_owned(),
                        ),
                        id,
                    ));

                    dependency_hash_map.insert(node_index, dependencies);
                }

                schema::Definition::TypeExtension(type_extension) => {
                    match type_extension {
                        schema::TypeExtension::Object(object_type_extension) => {
                            let id = object_type_extension.name.clone();
                            let dependencies = object_type_extension.get_dependencies();

                            let node_index = data.graph.add_node(Node::new(
                                Entity::new(
                                    dependencies.clone(),
                                    GraphQL::TypeExtension(GraphQLType::Object),
                                    object_type_extension.name,
                                    file.to_owned(),
                                    contents.to_owned(),
                                ),
                                get_extended_id(id),
                            ));

                            dependency_hash_map.insert(node_index, dependencies);
                        }

                        schema::TypeExtension::Scalar(scalar_type_extension) => {
                            let id = scalar_type_extension.name.clone();
                            let dependencies = scalar_type_extension.get_dependencies();

                            let node_index = data.graph.add_node(Node::new(
                                Entity::new(
                                    dependencies.clone(),
                                    GraphQL::TypeExtension(GraphQLType::Scalar),
                                    scalar_type_extension.name,
                                    file.to_owned(),
                                    contents.to_owned(),
                                ),
                                get_extended_id(id),
                            ));

                            dependency_hash_map.insert(node_index, dependencies);
                        }

                        schema::TypeExtension::Interface(interface_type_extension) => {
                            let id = interface_type_extension.name.clone();
                            let dependencies = interface_type_extension.get_dependencies();

                            let node_index = data.graph.add_node(Node::new(
                                Entity::new(
                                    dependencies.clone(),
                                    GraphQL::TypeExtension(GraphQLType::Scalar),
                                    interface_type_extension.name,
                                    file.to_owned(),
                                    contents.to_owned(),
                                ),
                                get_extended_id(id),
                            ));

                            dependency_hash_map.insert(node_index, dependencies);
                        }

                        schema::TypeExtension::Union(union_type_extension) => {
                            let id = union_type_extension.name.clone();
                            let dependencies = union_type_extension.get_dependencies();

                            let node_index = data.graph.add_node(Node::new(
                                Entity::new(
                                    dependencies.clone(),
                                    GraphQL::TypeExtension(GraphQLType::Union),
                                    union_type_extension.name,
                                    file.to_owned(),
                                    contents.to_owned(),
                                ),
                                get_extended_id(id),
                            ));

                            dependency_hash_map.insert(node_index, dependencies);
                        }

                        schema::TypeExtension::Enum(enum_type_extension) => {
                            let id = enum_type_extension.name.clone();
                            let dependencies = enum_type_extension.get_dependencies();

                            let node_index = data.graph.add_node(Node::new(
                                Entity::new(
                                    dependencies.clone(),
                                    GraphQL::TypeExtension(GraphQLType::Enum),
                                    enum_type_extension.name,
                                    file.to_owned(),
                                    contents.to_owned(),
                                ),
                                get_extended_id(id),
                            ));

                            dependency_hash_map.insert(node_index, dependencies);
                        }

                        schema::TypeExtension::InputObject(input_object_type_extension) => {
                            let id = input_object_type_extension.name.clone();
                            let dependencies = input_object_type_extension.get_dependencies();

                            let node_index = data.graph.add_node(Node::new(
                                Entity::new(
                                    dependencies.clone(),
                                    GraphQL::TypeExtension(GraphQLType::InputObject),
                                    input_object_type_extension.name,
                                    file.to_owned(),
                                    contents.to_owned(),
                                ),
                                get_extended_id(id),
                            ));

                            dependency_hash_map.insert(node_index, dependencies);
                        }
                    };
                }
            }
        }
    }

    // Populate the edges.
    for (node_index, dependencies) in dependency_hash_map {
        for dependency in dependencies {
            // https://docs.rs/petgraph/0.5.1/petgraph/graph/struct.Graph.html#method.node_indices
            let maybe_index = &data
                .graph
                .node_indices()
                .find(|index| data.graph[*index].id == dependency);

            if let Some(index) = *maybe_index {
                &data
                    .graph
                    .update_edge(index, node_index, (index, node_index));
            }
        }
    }

    Ok(())
}
