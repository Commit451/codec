package com.composevst

import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import java.io.BufferedReader
import java.io.InputStreamReader
import java.io.PrintWriter
import java.net.Socket

data class PluginState(
    val cutoff: Float = 1000f,
    val resonance: Float = 0f
)

class IpcClient(
    private val host: String = "127.0.0.1",
    private val port: Int = 9847
) {
    private val scope = CoroutineScope(Dispatchers.IO + SupervisorJob())

    private val _state = MutableStateFlow(PluginState())
    val state: StateFlow<PluginState> = _state

    private val _connected = MutableStateFlow(false)
    val connected: StateFlow<Boolean> = _connected

    private var socket: Socket? = null
    private var writer: PrintWriter? = null

    fun connect() {
        scope.launch {
            while (isActive) {
                try {
                    val sock = Socket(host, port)
                    sock.tcpNoDelay = true  // Disable Nagle's for low latency
                    socket = sock
                    writer = PrintWriter(sock.getOutputStream(), true)
                    _connected.value = true

                    val reader = BufferedReader(InputStreamReader(sock.getInputStream()))
                    while (isActive) {
                        val line = reader.readLine() ?: break
                        handleMessage(line)
                    }
                } catch (_: Exception) {
                    // Connection failed or lost
                } finally {
                    _connected.value = false
                    writer = null
                    socket?.close()
                    socket = null
                }
                // Retry after delay
                delay(1000)
            }
        }
    }

    /**
     * Parse incoming JSON manually to avoid Gson/reflection overhead at 30 updates/sec.
     * Messages are simple: {"type":"state","cutoff":1000.0,"resonance":0.5}
     */
    private fun handleMessage(json: String) {
        try {
            // Fast path: check if it's a state message without full parse
            if (!json.contains("\"state\"")) return

            val cutoff = extractFloat(json, "cutoff") ?: return
            val resonance = extractFloat(json, "resonance") ?: return
            _state.value = PluginState(cutoff, resonance)
        } catch (_: Exception) {
            // Ignore malformed messages
        }
    }

    /**
     * Extract a float value for a given key from a flat JSON string.
     * Avoids object allocation — just string scanning.
     */
    private fun extractFloat(json: String, key: String): Float? {
        val searchKey = "\"$key\":"
        val keyIdx = json.indexOf(searchKey)
        if (keyIdx == -1) return null

        val valueStart = keyIdx + searchKey.length
        // Skip whitespace
        var i = valueStart
        while (i < json.length && json[i] == ' ') i++

        // Read number chars
        val numStart = i
        while (i < json.length && (json[i].isDigit() || json[i] == '.' || json[i] == '-' || json[i] == 'E' || json[i] == 'e' || json[i] == '+')) i++

        if (i == numStart) return null
        return json.substring(numStart, i).toFloatOrNull()
    }

    /**
     * Send a parameter change to the plugin.
     * Writes directly on the IO thread — no coroutine-per-call overhead.
     */
    fun setParam(name: String, value: Float) {
        // Write directly — we're already called from a Compose callback on the main thread,
        // but PrintWriter is thread-safe and the write is tiny.
        // If the socket is gone, silently ignore.
        try {
            writer?.println("""{"type":"set_param","name":"$name","value":$value}""")
        } catch (_: Exception) {
            // Connection may be lost
        }
    }

    fun disconnect() {
        scope.cancel()
        socket?.close()
    }
}
