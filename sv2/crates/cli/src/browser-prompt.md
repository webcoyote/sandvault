# Browser

This session has a headless browser, started for it by sandvault (sv2), for
web browsing, testing and scraping. It speaks the Chrome DevTools Protocol
(CDP) at the address in the environment variable:

    $SV_BROWSER_ENDPOINT

Connect to it with Playwright, Puppeteer or any CDP client. Example
(Playwright):

```javascript
const { chromium } = require('playwright');
const browser = await chromium.connectOverCDP(process.env.SV_BROWSER_ENDPOINT);
const context = browser.contexts()[0] ?? await browser.newContext();
const page = await context.newPage();
await page.goto('https://example.com');
```

- Do not launch a browser of your own: connect to this one.
- `curl "$SV_BROWSER_ENDPOINT/json/version"` shows that it is running.
- Leave it running when you are done: it is shared by every script in this
  session. Playwright's `browser.close()` stops it; end the script instead.
  With Puppeteer, call `browser.disconnect()`, not `browser.close()`.
- It stops when this session ends.
