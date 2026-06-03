package com.composevst

import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import java.io.BufferedReader
import java.io.InputStreamReader
import java.io.PrintWriter
import java.net.Socket

/** Mirror of the plugin's granular parameters + meters. Defaults match the plugin. */
data class PluginState(
    val density: Float = 25f,
    val size: Float = 80f,
    val position: Float = 0.1f,
    val spray: Float = 0f,
    val pitch: Float = 0f,
    val pitchSpread: Float = 0f,
    val panSpread: Float = 0.5f,
    val feedback: Float = 0f,
    val mix: Float = 1f,
    val sync: Boolean = false,
    val reverse: Boolean = false,
    val division: Int = 4,
    val bpm: Float = 0f,
    val level: Float = 0f,
    val grains: Int = 0,
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
                    sock.tcpNoDelay = true
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
                delay(1000)
            }
        }
    }

    private fun handleMessage(json: String) {
        try {
            if (!json.contains("\"state\"")) return
            fun f(key: String, default: Float): Float = extractFloat(json, key) ?: default
            _state.value = PluginState(
                density = f("density", 25f),
                size = f("size", 80f),
                position = f("position", 0.1f),
                spray = f("spray", 0f),
                pitch = f("pitch", 0f),
                pitchSpread = f("pitch_spread", 0f),
                panSpread = f("pan_spread", 0.5f),
                feedback = f("feedback", 0f),
                mix = f("mix", 1f),
                sync = f("sync", 0f) >= 0.5f,
                reverse = f("reverse", 0f) >= 0.5f,
                division = f("division", 4f).toInt(),
                bpm = f("bpm", 0f),
                level = f("level", 0f),
                grains = f("grains", 0f).toInt(),
            )
        } catch (_: Exception) {
            // Ignore malformed messages
        }
    }

    /** Extract a numeric value for `key` from a flat JSON string (no allocation-heavy parse). */
    private fun extractFloat(json: String, key: String): Float? {
        val searchKey = "\"$key\":"
        val keyIdx = json.indexOf(searchKey)
        if (keyIdx == -1) return null

        var i = keyIdx + searchKey.length
        while (i < json.length && json[i] == ' ') i++

        val numStart = i
        while (i < json.length && (json[i].isDigit() || json[i] == '.' || json[i] == '-' || json[i] == 'E' || json[i] == 'e' || json[i] == '+')) i++

        if (i == numStart) return null
        return json.substring(numStart, i).toFloatOrNull()
    }

    /** Send a parameter change to the plugin (plain value; e.g. ms, semitones, or 0/1 for toggles). */
    fun setParam(name: String, value: Float) {
        try {
            writer?.println("""{"type":"set_param","name":"$name","value":$value}""")
        } catch (_: Exception) {
        }
    }

    /** Mark the start of an automation gesture so the host records the drag as one gesture. */
    fun beginGesture(name: String) = sendGesture(name, "begin")

    /** Mark the end of an automation gesture. */
    fun endGesture(name: String) = sendGesture(name, "end")

    private fun sendGesture(name: String, action: String) {
        try {
            writer?.println("""{"type":"gesture","name":"$name","action":"$action"}""")
        } catch (_: Exception) {
        }
    }

    fun disconnect() {
        scope.cancel()
        socket?.close()
    }
}
