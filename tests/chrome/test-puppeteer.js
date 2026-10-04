#!/usr/bin/env node
// Test headless Chrome via Puppeteer and CDP
//
// Dependencies are installed by the wrapper script before this runs.

const puppeteer = require('puppeteer-core');

const endpoint = process.env.SV_BROWSER_ENDPOINT;
if (!endpoint) {
    console.error('SV_BROWSER_ENDPOINT is not set. Run: sv --chrome shell');
    process.exit(1);
}

const VERBOSE = process.env.VERBOSE;
const verbose = VERBOSE === undefined ? 0 : /^\d+$/.test(VERBOSE) ? parseInt(VERBOSE, 10) : 1;
const log = (...args) => { if (verbose) console.log('  ', ...args); };

// Local fixture page: these tests verify browser plumbing (CDP, navigation,
// JS evaluation, rendering, DOM access), not any remote site's content. A
// data: URL keeps them deterministic -- example.com used to serve an <h1>,
// silently dropped it, and broke Test 4 in every browser suite.
const FIXTURE_HTML = `<!doctype html>
<html><head><title>Sandvault Test Page</title></head>
<body>
  <h1>Sandvault</h1>
  <textarea id="edit"></textarea>
</body></html>`;
const FIXTURE_URL = 'data:text/html,' + encodeURIComponent(FIXTURE_HTML);

if (verbose) console.log('test-puppeteer.js');
(async () => {
    log(`Connecting to ${endpoint}...`);
    const browser = await puppeteer.connect({ browserURL: endpoint });
    const version = await browser.version();
    log(`Connected: ${version}`);

    const page = await browser.newPage();

    // Test 1: Navigate to a page
    log('Test 1: Navigate to local fixture page...');
    await page.goto(FIXTURE_URL);
    const title = await page.title();
    log(`  Title: ${title}`);
    if (!title.includes('Sandvault Test Page')) {
        throw new Error(`Unexpected title: ${title}`);
    }
    log('  PASS');

    // Test 2: Evaluate JavaScript in the page
    log('Test 2: Evaluate JavaScript...');
    const userAgent = await page.evaluate(() => navigator.userAgent);
    log(`  User-Agent: ${userAgent}`);
    if (!userAgent) {
        throw new Error('No user agent returned');
    }
    log('  PASS');

    // Test 3: Take a screenshot (to verify rendering works)
    log('Test 3: Screenshot...');
    const screenshot = await page.screenshot();
    log(`  Screenshot size: ${screenshot.length} bytes`);
    if (screenshot.length === 0) {
        throw new Error('Empty screenshot');
    }
    log('  PASS');

    // Test 4: DOM access and text input
    log('Test 4: DOM manipulation...');
    const heading = await page.$eval('h1', el => el.textContent);
    log(`  H1 text: ${heading}`);
    if (!heading.includes('Sandvault')) {
        throw new Error(`Unexpected heading: ${heading}`);
    }
    // Typing into the textarea proves input events reach the page, not just
    // that the DOM can be read back.
    await page.type('#edit', 'hello sandvault');
    const typed = await page.$eval('#edit', el => el.value);
    log(`  Textarea value: ${typed}`);
    if (typed !== 'hello sandvault') {
        throw new Error(`Unexpected textarea value: ${typed}`);
    }
    log('  PASS');

    // Test 5: Real network access from inside the sandbox.
    // Asserts only on a non-empty title so a remote redesign cannot break it.
    log('Test 5: Network navigation...');
    await page.goto('https://example.com');
    const netTitle = await page.title();
    log(`  Title: ${netTitle}`);
    if (!netTitle) {
        throw new Error('No title from network navigation');
    }
    log('  PASS');

    await page.close();
    browser.disconnect();

    log('All Puppeteer tests passed.\n');
})().catch(err => {
    console.error(`  FAIL: ${err.message}`);
    process.exit(1);
});
