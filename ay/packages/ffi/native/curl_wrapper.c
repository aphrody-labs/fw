/*
 * curl_wrapper.c: C wrappers for bun:ffi.
 *
 * bun:ffi does not support variadic functions (curl_easy_setopt, curl_easy_getinfo), so fixed-signature
 * functions are provided that wrap the complete calls.
 *
 * Compiled with gcc by src/native.ts, linked with -lcurl.
 */

#include <curl/curl.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    char  *data;
    size_t size;
} Buffer;

static size_t write_cb(void *ptr, size_t size, size_t nmemb, Buffer *buf) {
    size_t realsize = size * nmemb;
    char *tmp = realloc(buf->data, buf->size + realsize + 1);
    if (!tmp) return 0;
    buf->data = tmp;
    memcpy(buf->data + buf->size, ptr, realsize);
    buf->size += realsize;
    buf->data[buf->size] = '\0';
    return realsize;
}

/*
 * curl_get: performs a GET on `url` and fills `buf` with the response.
 * Returns the response size in bytes, or -1 on error.
 * The caller must free(buf->data) after use.
 */
int curl_get(const char *url, Buffer *buf) {
    CURL *curl = curl_easy_init();
    if (!curl) return -1;

    buf->data = malloc(1);
    buf->size = 0;
    if (!buf->data) { curl_easy_cleanup(curl); return -1; }

    curl_easy_setopt(curl, CURLOPT_URL, url);
    curl_easy_setopt(curl, CURLOPT_WRITEFUNCTION, write_cb);
    curl_easy_setopt(curl, CURLOPT_WRITEDATA, buf);
    curl_easy_setopt(curl, CURLOPT_FOLLOWLOCATION, 1L);
    curl_easy_setopt(curl, CURLOPT_TIMEOUT, 30L);

    CURLcode res = curl_easy_perform(curl);
    curl_easy_cleanup(curl);

    if (res != CURLE_OK) {
        free(buf->data);
        buf->data = NULL;
        buf->size = 0;
        return -1;
    }
    return (int)buf->size;
}

/*
 * curl_get_status: returns the HTTP status code of a HEAD request on `url`.
 * Returns -1 on a cURL error.
 */
long curl_get_status(const char *url) {
    CURL *curl = curl_easy_init();
    if (!curl) return -1;

    curl_easy_setopt(curl, CURLOPT_URL, url);
    curl_easy_setopt(curl, CURLOPT_NOBODY, 1L);
    curl_easy_setopt(curl, CURLOPT_FOLLOWLOCATION, 1L);
    curl_easy_setopt(curl, CURLOPT_TIMEOUT, 10L);

    CURLcode res = curl_easy_perform(curl);
    long code = 0;
    if (res == CURLE_OK)
        curl_easy_getinfo(curl, CURLINFO_RESPONSE_CODE, &code);
    curl_easy_cleanup(curl);

    return (res == CURLE_OK) ? code : -1L;
}

/*
 * curl_get_response_code: performs a GET and returns the HTTP status code.
 * Useful to get the status of a URL while also fetching the body.
 */
long curl_get_response_code(const char *url) {
    CURL *curl = curl_easy_init();
    if (!curl) return -1;

    /* Sink that discards the response */
    Buffer sink = { NULL, 0 };
    sink.data = malloc(1);

    curl_easy_setopt(curl, CURLOPT_URL, url);
    curl_easy_setopt(curl, CURLOPT_WRITEFUNCTION, write_cb);
    curl_easy_setopt(curl, CURLOPT_WRITEDATA, &sink);
    curl_easy_setopt(curl, CURLOPT_FOLLOWLOCATION, 1L);
    curl_easy_setopt(curl, CURLOPT_TIMEOUT, 30L);

    CURLcode res = curl_easy_perform(curl);
    long code = 0;
    if (res == CURLE_OK)
        curl_easy_getinfo(curl, CURLINFO_RESPONSE_CODE, &code);
    curl_easy_cleanup(curl);
    if (sink.data) free(sink.data);

    return (res == CURLE_OK) ? code : -1L;
}

/*
 * buffer_data: returns the pointer to the data of the Buffer.
 */
const char *buffer_data(Buffer *buf) {
    return buf->data;
}

/*
 * buffer_free: frees the memory of a Buffer filled by curl_get.
 */
void buffer_free(Buffer *buf) {
    if (buf->data) {
        free(buf->data);
        buf->data = NULL;
        buf->size = 0;
    }
}
