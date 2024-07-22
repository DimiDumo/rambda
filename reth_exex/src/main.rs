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
    let channel_b = conn.create_channel().await?;

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

    let confirm = channel_a
        .basic_publish(
            "",
            QUEUE_NAME,
            BasicPublishOptions::default(),
            b"Yo hey whats up from ExEx",
            BasicProperties::default(),
        )
        .await?
        .await?;

    info!("confirm: {:?}", confirm);


    while let Some(notification) = ctx.notifications.recv().await {
        match &notification {
            ExExNotification::ChainCommitted { new } => {
                info!(committed_chain = ?new.range(), "Received commit");
                let json = serde_json::to_string(&new).map_err(|err| {
                    info!("Failed to parse json");
                    eyre!(err)
                })?;
                info!("parsed json successfully");

                let json_length = json.len();
                let json_size_in_mb = json_length as f64 / (1024.0 * 1024.0);

                info!("JSON length: {} characters", json_length);
                info!("JSON size: {:.2} MB", json_size_in_mb);

                if json_size_in_mb < 12. {
                    // redis::cmd("PUBLISH").arg("chain_committed").arg(json).query_async(&mut con).await.map_err(|err| {
                    //     info!("Failed to publish json to redis");
                    //     eyre!(err)
                    // })?;
                    info!("Published chain_committed");
                } else {
                    info!("Skipping");
                }
            }
            ExExNotification::ChainReorged { old, new } => {
                info!(from_chain = ?old.range(), to_chain = ?new.range(), "Received reorg");
                let json = serde_json::to_string(&new).map_err(|err| {
                    info!("Failed to parse json");
                    eyre!(err)
                })?;
                info!("parsed json successfully");

                let json_length = json.len();
                let json_size_in_mb = json_length as f64 / (1024.0 * 1024.0);

                info!("JSON length: {} characters", json_length);
                info!("JSON size: {:.2} MB", json_size_in_mb);

                if json_size_in_mb < 12. {
                    // redis::cmd("PUBLISH").arg("chain_reorged").arg(json).query_async(&mut con).await.map_err(|err| {
                    //     info!("Failed to publish json to redis");
                    //     eyre!(err)
                    // })?;
                    info!("Published chain_reorged")
                } else {
                    info!("Skipping");
                }
            }
            ExExNotification::ChainReverted { old } => {
                info!(reverted_chain = ?old.range(), "Received revert");
                let json = serde_json::to_string(&old).map_err(|err| {
                    info!("Failed to parse json");
                    eyre!(err)
                })?;
                info!("parsed json successfully");

                let json_length = json.len();
                let json_size_in_mb = json_length as f64 / (1024.0 * 1024.0);

                info!("JSON length: {} characters", json_length);
                info!("JSON size: {:.2} MB", json_size_in_mb);

                if json_size_in_mb < 12. {
                    // redis::cmd("PUBLISH").arg("chain_reverted").arg(json).query_async(&mut con).await.map_err(|err| {
                    //     info!("Failed to publish json to redis");
                    //     eyre!(err)
                    // })?;
                    info!("Published chain_reverted")
                } else {
                    info!("Skipping");
                }
            }
        };

        if let Some(committed_chain) = notification.committed_chain() {
            ctx.events.send(ExExEvent::FinishedHeight(committed_chain.tip().number))?;
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
