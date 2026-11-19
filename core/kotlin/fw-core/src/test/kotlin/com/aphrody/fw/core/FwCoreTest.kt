package dev.aphrody.fw.core

import com.sun.net.httpserver.HttpServer
import kotlinx.coroutines.test.runTest
import java.net.InetSocketAddress
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class FwCoreTest {
    @Test
    fun buildsAllCanonicalEndpoints() {
        val client = FwClient(FwEndpointConfig(baseUrl = "http://127.0.0.1:4379/"))
        assertEquals("http://127.0.0.1:4379/health", client.healthUrl())
        assertEquals("http://127.0.0.1:4379/openapi.json", client.openApiUrl())
        assertEquals("http://127.0.0.1:4379/.well-known/agent.json", client.agentCardUrl())
        assertEquals("http://127.0.0.1:4379/v1/chat/completions", client.chatCompletionsUrl())
        assertEquals("http://127.0.0.1:4379/mcp", client.mcpUrl())
        assertEquals("http://127.0.0.1:4379/acp", client.acpUrl())
        assertEquals("http://127.0.0.1:4379/a2a", client.a2aUrl())
        assertTrue(client.vfsListUrl("/crates/fw-bin").contains("%2Fcrates%2Ffw-bin"))
    }

    @Test
    fun roundTripsHealthAndMcpOverLocalHttp() = runTest {
        val server = HttpServer.create(InetSocketAddress("127.0.0.1", 0), 0)
        server.createContext("/health") { ex ->
            val body = """{"ok":true,"product":"fw","version":"4.2.0","protocol":"openai+mcp+acp+a2a"}""".toByteArray()
            ex.responseHeaders.add("Content-Type", "application/json")
            ex.sendResponseHeaders(200, body.size.toLong())
            ex.responseBody.use { it.write(body) }
        }
        server.createContext("/mcp") { ex ->
            val body = """{"jsonrpc":"2.0","id":"1","result":"pong"}""".toByteArray()
            ex.responseHeaders.add("Content-Type", "application/json")
            ex.sendResponseHeaders(200, body.size.toLong())
            ex.responseBody.use { it.write(body) }
        }
        server.start()
        try {
            val port = server.address.port
            val client = FwClient(FwEndpointConfig(baseUrl = "http://127.0.0.1:$port"))
            val health = client.health()
            assertTrue(health.ok)
            assertEquals("fw", health.product)

            val mcp = client.callMcp(McpJsonRpcRequest(id = "1", method = "tools/list"))
            assertEquals("pong", mcp.result)
        } finally {
            server.stop(0)
        }
    }
}
