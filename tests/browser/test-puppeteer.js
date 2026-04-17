#!/usr/bin/env node
// Test headless Chrome via Puppeteer and CDP
//
// Usage: sv --browser shell -- node tests/browser/test-puppeteer.js
//    or: node tests/browser/test-puppeteer.js  (from inside sv --browser shell)

// Auto-install dependencies if needed
const path = require('path');
const testDir = path.dirname(require.resolve('./package.json'));
try { require.resolve('puppeteer-core'); } catch {
    const { execSync } = require('child_process');
    console.log('Installing dependencies...');
    execSync('npm install --no-audit --no-fund --no-update-notifier', { cwd: testDir, stdio: 'inherit' });
}

const puppeteer = require('puppeteer-core');

const endpoint = process.env.SV_BROWSER_ENDPOINT;
if (!endpoint) {
    console.error('SV_BROWSER_ENDPOINT is not set. Run: sv --browser shell');
    process.exit(1);
}

const VERBOSE = process.env.VERBOSE;
const verbose = VERBOSE === undefined ? 0 : /^\d+$/.test(VERBOSE) ? parseInt(VERBOSE, 10) : 1;
const log = (...args) => { if (verbose) console.log('  ', ...args); };

if (verbose) console.log('test-puppeteer.js');
(async () => {
    log(`Connecting to ${endpoint}...`);
    const browser = await puppeteer.connect({ browserURL: endpoint });
    const version = await browser.version();
    log(`Connected: ${version}`);

    const page = await browser.newPage();

    // Test 1: Navigate to a page
    log('Test 1: Navigate to example.com...');
    await page.goto('https://example.com');
    const title = await page.title();
    log(`  Title: ${title}`);
    if (!title.includes('Example')) {
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

    // Test 4: DOM manipulation
    log('Test 4: DOM manipulation...');
    const heading = await page.$eval('h1', el => el.textContent);
    log(`  H1 text: ${heading}`);
    if (!heading.includes('Example')) {
        throw new Error(`Unexpected heading: ${heading}`);
    }
    log('  PASS');

    await page.close();
    browser.disconnect();

    log('All Puppeteer tests passed.\n');
})().catch(err => {
    console.error(`  FAIL: ${err.message}`);
    process.exit(1);
});
