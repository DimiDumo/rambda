import { ExEx } from './index.js';
import { promises as fs } from 'fs';

async function main() {
  console.time('init');
  let data;
  try {
    console.log('Reading data file');
    data = await fs.readFile('data.json');
    console.log('Parsing data file');
    data = JSON.parse(data);
  } catch (err) {
    console.timeEnd('init');
    console.error(
      'Failed to initialize ExEx function before calling users function: ',
      err,
    );
    return;
  }
  console.timeEnd('init');
  try {
    console.time('exex');
    await ExEx(data);
    console.timeEnd('exex');
  } catch (err) {
    console.timeEnd('exex');
    console.error('Error in main: ', err);
  }
}

// Run test func with test data
main();
