use futures::Future;
use reth_exex::{ExExContext, ExExEvent, ExExNotification};
use reth_node_api::FullNodeComponents;
use reth_node_ethereum::EthereumNode;
use reth_tracing::tracing::info;
// use redis::{Client, Commands};
use serde_json;
use eyre::eyre;
use std::time::{SystemTime, UNIX_EPOCH};
use futures::executor::block_on;
use lapin::{
    options::*, publisher_confirm::Confirmation, types::FieldTable, BasicProperties, Connection,
    ConnectionProperties,
};
use serde::ser::{Serialize, SerializeMap, Serializer};
use serde_json;
use serde_json::Error as JsonError;
use std::any::type_name;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;

struct ExExNotificationWrapper<'a>(&'a ExExNotification);

impl<'a> Serialize for ExExNotificationWrapper<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let notification = self.0;

        if let Some(chain_committed) = notification.committed_chain() {
            let mut map = serializer.serialize_map(Some(1))?;

            let mut chain_committed_map = BTreeMap::new();
            let mut new_chain_map = BTreeMap::new();

            let blocks: BTreeMap<String, _> = chain_committed
                .blocks()
                .iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect();

            new_chain_map.insert("blocks", &blocks);
            chain_committed_map.insert("new", &new_chain_map);

            map.serialize_entry("ChainCommitted", &chain_committed_map)?;

            map.end()
        } else {
            // Handle other variants of ExExNotification
            // For now, we'll just serialize an empty map
            let map = serializer.serialize_map(Some(0))?;
            map.end()
        }
    }
}

/// The initialization logic of the ExEx is just an async function.
///
/// During initialization you can wait for resources you need to be up for the ExEx to function,
/// like a database connection.
async fn exex_init<Node: FullNodeComponents>(
    ctx: ExExContext<Node>,
) -> eyre::Result<impl Future<Output = eyre::Result<()>>> {
    Ok(exex(ctx))
}

/// An ExEx is just a future, which means you can implement all of it in an async function!
///
/// This ExEx just prints out whenever either a new chain of blocks being added, or a chain of
/// blocks being re-orged. After processing the chain, emits an [ExExEvent::FinishedHeight] event.
async fn exex<Node: FullNodeComponents>(mut ctx: ExExContext<Node>) -> eyre::Result<()> {
    // info!("Connecting to redis");
    // let client = Client::open("redis://localhost:6379")?;
    // info!("Connected to redis successfully");
    // let mut con = client.get_multiplexed_tokio_connection().await?;
    // info!("Got redis connection");
    // redis::cmd("PUBLISH").arg("init").arg("yo diggi 2").query_async(&mut con).await?;
    // info!("Published init message");


    let conn = Connection::connect(
        "amqp://user:password@localhost:5672",
        ConnectionProperties::default(),
    )
    .await?;

    info!("CONNECTED to rabbitmq");

    let channel_a = conn.create_channel().await?;

    let QUEUE_NAME = "exex";

    let queue = channel_a
        .queue_declare(
            QUEUE_NAME,
            QueueDeclareOptions {
                durable: true,
                ..Default::default()
            },
            FieldTable::default(),
        )
        .await?;

    info!(?queue, "Declared queue");

    // let confirm = channel_a
    //     .basic_publish(
    //         "",
    //         QUEUE_NAME,
    //         BasicPublishOptions::default(),
    //         b"{\"hi\": \"from exex\"}",
    //         BasicProperties::default(),
    //     )
    //     .await?
    //     .await?;

    // info!("confirm: {:?}", confirm);


    while let Some(notification) = ctx.notifications.recv().await {
        if let Some(committed_chain) = notification.committed_chain() {
            println!("Sent committed chain event");
            ctx.events
                .send(ExExEvent::FinishedHeight(committed_chain.tip().number))?;
        }


        match serde_json::to_string(&ExExNotificationWrapper(&notification)) {
            Ok(json) => {
                let json_length = json.len();
                let json_size_in_mb = json_length as f64 / (1024.0 * 1024.0);

                if json_size_in_mb < 12. {
                    let confirm = channel_a
                        .basic_publish(
                            "",
                            QUEUE_NAME,
                            BasicPublishOptions::default(),
                            json.as_bytes(),
                            BasicProperties::default(),
                        )
                        .await?
                        .await?;

                    info!("Pushed to queue: {:?}", confirm);
                } else {
                    info!("Skipping");
                }
            }
            Err(e) => {
                eprintln!("Failed to serialize notification: {}", e);
                log_detailed_error(&notification, &e);
            }
        }
    }

    Ok(())
}

fn main() -> eyre::Result<()> {
    reth::cli::Cli::parse_args().run(|builder, _| async move {
        let handle = builder
            .node(EthereumNode::default())
            .install_exex("Minimal", exex_init)
            .launch()
            .await?;

        handle.wait_for_node_exit().await
    })
}

#[cfg(test)]
mod tests {
    use reth_execution_types::{Chain, ExecutionOutcome};
    use reth_exex_test_utils::{test_exex_context, PollOnce};
    use std::pin::pin;

    #[tokio::test]
    async fn test_exex() -> eyre::Result<()> {
        // Initialize a test Execution Extension context with all dependencies
        let (ctx, mut handle) = test_exex_context().await?;

        // Save the current head of the chain to check the finished height against it later
        let head = ctx.head;

        // Send a notification to the Execution Extension that the chain has been committed
        handle
            .send_notification_chain_committed(Chain::from_block(
                handle.genesis.clone(),
                ExecutionOutcome::default(),
                None,
            ))
            .await?;

        // Initialize the Execution Extension
        let mut exex = pin!(super::exex_init(ctx).await?);

        // Check that the Execution Extension did not emit any events until we polled it
        handle.assert_events_empty();

        // Poll the Execution Extension once to process incoming notifications
        exex.poll_once().await?;

        // Check that the Execution Extension emitted a `FinishedHeight` event with the correct
        // height
        handle.assert_event_finished_height(head.number)?;

        Ok(())
    }
}
