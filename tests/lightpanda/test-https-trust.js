#!/usr/bin/env node
// Test that Lightpanda inside the sandbox trusts the same TLS roots as macOS.
//
// This test depends on the network on purpose: what it checks is TLS
// certificate verification against a real site's chain, which a local
// fixture cannot reproduce (it would need a CA in the system trust store).
//
// start-lightpanda used to pass `--ca-cert /etc/ssl/cert.pem`, which makes
// Lightpanda trust only that bundle. The bundle lacks the root of
// example.com's chain (SSL.com TLS ECC Root CA 2022, cross-signed by AAA
// Certificate Services, as of 2026-11), so navigation failed with
// PeerFailedVerification while sites with other roots (captive.apple.com,
// github.com) still loaded. example.com is used because its chain exposes
// that bug; if its chain changes to a root in the bundle this test stops
// detecting a regression, but it will not fail spuriously. example.com asks
// not to be used in CI (see test-playwright.js); this suite does not run in
// CI, since it needs a real `sv --lightpanda` session, and makes one request.
//
// Dependencies are installed by the wrapper script before this runs.

const { chromium } = require('playwright-core');

const endpoint = process.env.SV_BROWSER_ENDPOINT;
if (!endpoint) {
    console.error('SV_BROWSER_ENDPOINT is not set. Run: sv --lightpanda shell');
    process.exit(1);
}

const VERBOSE = process.env.VERBOSE;
const verbose = VERBOSE === undefined ? 0 : /^\d+$/.test(VERBOSE) ? parseInt(VERBOSE, 10) : 1;
const log = (...args) => { if (verbose) console.log('  ', ...args); };

const URL = 'https://example.com/';

if (verbose) console.log('test-https-trust.js');
(async () => {
    log(`Connecting to ${endpoint}...`);
    const browser = await chromium.connectOverCDP(endpoint);

    const context = await browser.newContext();
    const page = await context.newPage();

    log(`Test 1: HTTPS navigation to ${URL} verifies its certificate...`);
    try {
        await page.goto(URL, { timeout: 30000 });
    } catch (err) {
        const first = String(err.message).split('\n')[0];
        if (first.includes('PeerFailedVerification')) {
            throw new Error(`${first} (Lightpanda does not trust the system TLS roots; was it started with --ca-cert or --ca-path?)`);
        }
        throw err;
    }
    // Assert only on a non-empty title so a remote redesign cannot break it.
    const title = await page.title();
    log(`  Title: ${title}`);
    if (!title) {
        throw new Error(`No title from ${URL}`);
    }
    log('  PASS');

    await context.close();
    browser.close();

    log('All HTTPS trust tests passed.\n');
})().catch(err => {
    console.error(`  FAIL: ${err.message}`);
    process.exit(1);
});
