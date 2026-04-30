# Browser Automation

A headless browser (Chrome or Lightpanda, depending on how `sv` was started) is available for web browsing, testing, and scraping.

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
- Lightpanda's CDP coverage is narrower than Chrome's; if a script fails on Lightpanda, ask the user to relaunch with `sv --chrome` (or just `--browser`) instead of `--lightpanda`.
