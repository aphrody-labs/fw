5.3.0
=====

## 🐞 Bug fixes

- Fixed HTTP header conflict between Content-Length and Transfer-Encoding in res.send - by [@YuryShkoda](https://github.com/YuryShkoda) in [#4893](https://github.com/expressjs/express/pull/4893)


    Fixed the behavior of `res.send()` to prevent conflicts between `Content-Length` and `Transfer-Encoding` HTTP headers in responses. The `Content-Length` header in `res.send()` is now only added when a `Transfer-Encoding` header is not present, complying with the HTTP specification that states both headers should not coexist in the same response. ETag generation is unaffected by the presence of a `Transfer-Encoding` header - by [@cuishuang](https://github.com/cuishuang) in [#7459](https://github.com/expressjs/express/pull/7459)

* Upgrade `qs` to `^6.16.0`, which fixes [CVE-2026-2391](https://www.cve.org/CVERecord?id=CVE-2026-2391) ([GHSA-w7fw-mjwx-w883](https://github.com/ljharb/qs/security/advisories/GHSA-w7fw-mjwx-w883)), [CVE-2026-82417](https://www.cve.org/CVERecord?id=CVE-2026-82417) ([GHSA-4mjr-xmp4-gh2g](https://github.com/advisories/GHSA-4mjr-xmp4-gh2g)) and [CVE-2026-82562](https://www.cve.org/CVERecord?id=CVE-2026-82562) ([GHSA-x5fp-wj9c-mxmx](https://github.com/advisories/GHSA-x5fp-wj9c-mxmx)): `arrayLimit` bypasses in comma parsing and a denial of service via an attacker-controlled `isBuffer` - by [@davetashner](https://github.com/davetashner) in [#7057](https://github.com/expressjs/express/pull/7057) and [#7478](https://github.com/expressjs/express/pull/7478), and [@cyphercodes](https://github.com/cyphercodes) in [#7305](https://github.com/expressjs/express/pull/7305)

* Upgrade `proxy-addr` to `^2.0.8`, which fixes [CVE-2026-90711](https://www.cve.org/CVERecord?id=CVE-2026-90711) - by [@lazerg](https://github.com/lazerg) in [#7474](https://github.com/expressjs/express/pull/7474)

## 🚀 Improvements

* Allow conditional revalidation for QUERY requests. `req.fresh` previously only validated freshness for GET and HEAD requests, so QUERY responses never returned 304 despite a matching validator. Since QUERY is a safe, idempotent, and cacheable method that supports conditional requests, it is now included in the freshness check - by [@Cherry](https://github.com/Cherry) in [#7366](https://github.com/expressjs/express/pull/7366)

    ```js
    // QUERY /reports with If-None-Match: "12345"
    app.query('/reports', (req, res) => {
      res.set('ETag', '"12345"');
      res.send(results); // now responds 304 Not Modified
    });
    ```

* Improve HTML structure in `res.redirect()` responses when HTML format is accepted by adding `<!DOCTYPE html>`, `<title>`, and `<body>` tags for better browser compatibility - by [@Bernice55231](https://github.com/Bernice55231) in [#5167](https://github.com/expressjs/express/pull/5167)

* When calling `app.render` with options set to null, the locals object is handled correctly, preventing unexpected errors and making the method behave the same as when options is omitted or an empty object is passed - by [AkaHarshit](https://github.com/AkaHarshit) in [#6903](https://github.com/expressjs/express/pull/6903)

    ```js
    app.render('index', null, callback); // now works as expected
    ```

* Upgrade `content-type` to `^2.0.0`, bringing a faster parser (~1.5x quicker `Content-Type` parsing/formatting in `res.send()`) along with a behavior change: `res.send()` now keeps any existing parameters when adding the charset and no longer throws on a `Content-Type` that fails to parse. `type-is` is upgraded to `^2.1.0` as part of the same change - by [@blakeembrey](https://github.com/blakeembrey) in [#7234](https://github.com/expressjs/express/pull/7234)

    ```js
    res.set('Content-Type', 'text/plain; foo=bar').send('hey');
    // -> Content-Type: text/plain; foo=bar; charset=utf-8
    ```

* The default error handler now logs the full error object instead of only its stack trace, so nested details such as `Error.cause` and library-specific properties (e.g. Sequelize's `parent`/`original`) are no longer swallowed - by [@Nitin-Mohapatra](https://github.com/Nitin-Mohapatra) in [#6464](https://github.com/expressjs/express/pull/6464)

* Upgrade `content-disposition` to `^2.0.1`, which changes the `Content-Disposition` header emitted by `res.download()`, `res.attachment()`, and `res.sendFile()`: file names that are valid HTTP tokens are no longer wrapped in quotes. This is equivalent per RFC 6266, but applications asserting on the exact header bytes should update their expectations - by [@blakeembrey](https://github.com/blakeembrey) in [#7233](https://github.com/expressjs/express/pull/7233)

    ```js
    res.attachment('user.html');
    // before -> Content-Disposition: attachment; filename="user.html"
    // after  -> Content-Disposition: attachment; filename=user.html
    ```

* Upgrade `body-parser` to `^2.3.0`, which fixes [CVE-2026-12590](https://www.cve.org/CVERecord?id=CVE-2026-12590) ([GHSA-v422-hmwv-36x6](https://github.com/expressjs/body-parser/security/advisories/GHSA-v422-hmwv-36x6)): an invalid `limit` option value caused request body size enforcement to be silently disabled (fail-open), allowing a denial of service via arbitrarily large payloads. Invalid `limit` values now throw at parser initialization instead of being ignored - by [@Mayvis](https://github.com/Mayvis) in [#7390](https://github.com/expressjs/express/pull/7390)

## ⚡ Performance

* Avoid duplicate Content-Type header processing in `res.send()` when sending string responses without an explicit Content-Type header - by [@bjohansebas](https://github.com/bjohansebas) in [#6991](https://github.com/expressjs/express/pull/6991)

5.2.1
=====

