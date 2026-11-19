package com.aphrody.fw.sdk

/**
 * Fw Kotlin Multiplatform-ready SDK client contract for
 * OpenAI-compatible HTTP API, MCP, ACP, A2A, VFS OS, and Brain endpoints.
 */
data class FwEndpointConfig(
    val baseUrl: String = "http://127.0.0.1:4379",
    val timeoutMs: Long = 30_000L,
    val protocol: String = "openai+mcp+acp+a2a"
)

class FwSdk(val config: FwEndpointConfig = FwEndpointConfig()) {
    fun healthUrl(): String = "${config.baseUrl}/health"
    fun openApiUrl(): String = "${config.baseUrl}/openapi.json"
    fun agentCardUrl(): String = "${config.baseUrl}/.well-known/agent.json"
    fun chatCompletionsUrl(): String = "${config.baseUrl}/v1/chat/completions"
    fun mcpUrl(): String = "${config.baseUrl}/mcp"
    fun acpUrl(): String = "${config.baseUrl}/acp"
    fun a2aUrl(): String = "${config.baseUrl}/a2a"
    fun vfsListUrl(path: String = "/"): String = "${config.baseUrl}/vfs/ls?path=$path"
}
