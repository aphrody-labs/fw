package dev.aphrody.fw.core

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * Endpoint and transport configuration for connecting any `aphrody-labs` Kotlin,
 * Android, or Compose Multiplatform app to a `fw` daemon or edge worker.
 */
@Serializable
data class FwEndpointConfig(
    val baseUrl: String = "http://127.0.0.1:4379",
    val timeoutMs: Long = 30_000L,
    val protocol: String = "openai+mcp+acp+a2a",
    val apiKey: String? = null,
)

@Serializable
data class FwHealthResponse(
    val ok: Boolean,
    val product: String = "fw",
    val version: String = "4.2.0",
    val protocol: String = "openai+mcp+acp+a2a",
)

@Serializable
data class McpJsonRpcRequest(
    val jsonrpc: String = "2.0",
    val id: String,
    val method: String,
    val params: Map<String, String> = emptyMap(),
)

@Serializable
data class McpJsonRpcResponse(
    val jsonrpc: String = "2.0",
    val id: String,
    val result: String? = null,
    val error: String? = null,
)

@Serializable
data class ChatMessage(
    val role: String,
    val content: String,
)

@Serializable
data class ChatCompletionRequest(
    val model: String,
    val messages: List<ChatMessage>,
    val temperature: Double = 0.2,
    @SerialName("max_tokens")
    val maxTokens: Int = 1024,
)

@Serializable
data class ChatChoice(
    val index: Int = 0,
    val message: ChatMessage,
    @SerialName("finish_reason")
    val finishReason: String = "stop",
)

@Serializable
data class ChatCompletionResponse(
    val id: String,
    val model: String,
    val choices: List<ChatChoice>,
)

@Serializable
data class VfsEntry(
    val path: String,
    val isDir: Boolean = false,
    val bytes: Long = 0L,
)

@Serializable
data class AphrodyLabManifest(
    val org: String = "aphrody-labs",
    val mavenGroup: String = "dev.aphrody.fw",
    val version: String = "4.2.0",
    val modules: List<String> = listOf("fw-core", "fw-material3", "fw-rust-ffi"),
)
