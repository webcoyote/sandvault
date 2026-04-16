# Browser Automation

A headless Chrome browser is available for web browsing, testing, and scraping.

Connect via the Chrome DevTools Protocol (CDP) at:

    $SV_BROWSER_ENDPOINT

Use Puppeteer, Playwright, or any CDP-compatible library. Example (Puppeteer):

```javascript
const browser = await puppeteer.connect({
  browserWSEndpoint: (await (await fetch(`${process.env.SV_BROWSER_ENDPOINT}/json/version`)).json()).webSocketDebuggerUrl,
});
const page = await browser.newPage();
await page.goto('https://example.com');
```

Important:
- Do NOT launch a new browser; connect to the existing one.
- Use `curl $SV_BROWSER_ENDPOINT/json/version` to verify the browser is running.
