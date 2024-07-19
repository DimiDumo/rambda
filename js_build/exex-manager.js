import { ExEx } from './index.js';
import amqp from 'amqplib/callback_api';

function main() {
  console.log('Starting main');
  try {
    ExEx('Hi!');

    subscribe();
  } catch (err) {
    console.error('error in main: ', err);
  }
}

// Start main thread
main();

const RABBITMQ_URL =
  process.env.RABBITMQ_URL || 'amqp://user:password@localhost:5672';

// Subscribe to events from RabbitMQ
function subscribe() {
  // Replace with your RabbitMQ server URL
  const QUEUE_NAME = 'test_queue';

  amqp.connect(RABBITMQ_URL, (connectError, connection) => {
    if (connectError) {
      throw connectError;
    }

    connection.createChannel((channelError, channel) => {
      if (channelError) {
        throw channelError;
      }

      console.log('connected to channel: ', channel);

      channel.assertQueue(QUEUE_NAME, {
        durable: true,
      });

      console.log(
        `[*] Waiting for messages in ${QUEUE_NAME}. To exit press CTRL+C`,
      );

      channel.consume(QUEUE_NAME, (msg) => {
        if (msg !== null) {
          console.log(`[x] Received ${msg.content.toString()}`);
          channel.ack(msg);
        }
      });
    });
  });
}
