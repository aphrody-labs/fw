package dev.aphrody.fw.core

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import java.net.URI
import java.net.URLEncoder
import java.net.http.HttpClient
import java.net.http.HttpRequest
import java.net.http.HttpResponse
import java.nio.charset.StandardCharsets
import java.time.Duration

/**
 * Coroutine-native HTTP client for `fw` and all `aphrody-labs` services.
 * Zero third-party HTTP dependencies — uses JDK `java.net.http.HttpClient` + `kotlinx.serialization`.
 */
class FwClient(
    val config: FwEndpointConfig = FwEndpointConfig(),
    private val json: Json = Json { ignoreUnknownKeys = true; encodeDefaults = true },
) {
    private val http: HttpClient = HttpClient.newBuilder()
        .connectTimeout(Duration.ofMillis(config.timeoutMs))
        .build()

    fun healthUrl(): String = "${config.baseUrl.trimEnd('/')}/health"
    fun openApiUrl(): String = "${config.baseUrl.trimEnd('/')}/openapi.json"
    fun agentCardUrl(): String = "${config.baseUrl.trimEnd('/')}/.well-known/agent.json"
    fun chatCompletionsUrl(): String = "${config.baseUrl.trimEnd('/')}/v1/chat/completions"
    fun mcpUrl(): String = "${config.baseUrl.trimEnd('/')}/mcp"
    fun acpUrl(): String = "${config.baseUrl.trimEnd('/')}/acp"
    fun a2aUrl(): String = "${config.baseUrl.trimEnd('/')}/a2a"
    fun vfsListUrl(path: String = "/"): String {
        val encoded = URLEncoder.encode(path, StandardCharsets.UTF_8)
        return "${config.baseUrl.trimEnd('/')}/vfs/ls?path=$encoded"
    }

    private fun baseRequest(url: String): HttpRequest.Builder {
        val builder = HttpRequest.newBuilder()
            .uri(URI.create(url))
            .timeout(Duration.ofMillis(config.timeoutMs))
            .header("Accept", "application/json")
        config.apiKey?.let { builder.header("Authorization", "Bearer $it") }
        return builder
    }

    suspend fun health(): FwHealthResponse = withContext(Dispatchers.IO) {
        val req = baseRequest(healthUrl()).GET().build()
        val resp = http.send(req, HttpResponse.BodyHandlers.ofString())
        check(resp.statusCode() in 200..299) { "fw /health HTTP ${resp.statusCode()}: ${resp.body()}" }
        json.decodeFromString<FwHealthResponse>(resp.body())
    }

    suspend fun chatCompletion(request: ChatCompletionRequest): ChatCompletionResponse =
        withContext(Dispatchers.IO) {
            val body = json.encodeToString(request)
            val req = baseRequest(chatCompletionsUrl())
                .header("Content-Type", "application/json")
                .POST(HttpRequest.BodyPublishers.ofString(body))
                .build()
            val resp = http.send(req, HttpResponse.BodyHandlers.ofString())
            check(resp.statusCode() in 200..299) { "fw /v1/chat/completions HTTP ${resp.statusCode()}: ${resp.body()}" }
            json.decodeFromString<ChatCompletionResponse>(resp.body())
        }

    suspend fun callMcp(request: McpJsonRpcRequest): McpJsonRpcResponse =
        withContext(Dispatchers.IO) {
            val body = json.encodeToString(request)
            val req = baseRequest(mcpUrl())
                .header("Content-Type", "application/json")
                .POST(HttpRequest.BodyPublishers.ofString(body))
                .build()
            val resp = http.send(req, HttpResponse.BodyHandlers.ofString())
            check(resp.statusCode() in 200..299) { "fw /mcp HTTP ${resp.statusCode()}: ${resp.body()}" }
            json.decodeFromString<McpJsonRpcResponse>(resp.body())
        }
}
