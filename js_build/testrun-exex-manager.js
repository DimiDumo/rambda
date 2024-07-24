import { ExEx } from './index.js';

const testData = {
  block: 'block data',
};

function main(data) {
  console.log('Starting main with: ', testData);
  try {
    ExEx('Hi!');
  } catch (err) {
    console.error('Error in main: ', err);
  }
}

// Run test func with test data
main();
