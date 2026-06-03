package com.composevst

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Window
import androidx.compose.ui.window.application
import androidx.compose.ui.window.rememberWindowState
import com.composevst.components.FrequencyResponsePlot
import com.composevst.components.ParamSlider
import kotlin.math.roundToInt

fun main() = application {
    val windowState = rememberWindowState(size = DpSize(480.dp, 620.dp))

    Window(
        onCloseRequest = ::exitApplication,
        title = "Compose VST - Low Pass Filter",
        state = windowState,
        resizable = true
    ) {
        ComposeVstApp()
    }
}

@Composable
fun ComposeVstApp() {
    val client = remember { IpcClient() }
    val pluginState by client.state.collectAsState()
    val isConnected by client.connected.collectAsState()

    // Local state for when UI is driving changes (avoids jitter)
    var localCutoff by remember { mutableStateOf<Float?>(null) }
    var localResonance by remember { mutableStateOf<Float?>(null) }
    var localSweep by remember { mutableStateOf<Float?>(null) }

    val cutoff = localCutoff ?: pluginState.cutoff
    val resonance = localResonance ?: pluginState.resonance
    val sweep = localSweep ?: pluginState.sweep

    // Sync from plugin when not actively dragging
    LaunchedEffect(pluginState) {
        localCutoff = null
        localResonance = null
        localSweep = null
    }

    LaunchedEffect(Unit) {
        client.connect()
    }

    DisposableEffect(Unit) {
        onDispose { client.disconnect() }
    }

    MaterialTheme(
        colorScheme = darkColorScheme()
    ) {
        Surface(
            modifier = Modifier.fillMaxSize(),
            color = MaterialTheme.colorScheme.background
        ) {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(16.dp),
                horizontalAlignment = Alignment.CenterHorizontally
            ) {
                // Header
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Column {
                        Text(
                            "Low Pass Filter",
                            style = MaterialTheme.typography.headlineSmall,
                            color = MaterialTheme.colorScheme.onBackground
                        )
                        Text(
                            text = if (pluginState.bpm > 0f)
                                "♩ = %.1f BPM".format(pluginState.bpm)
                            else
                                "No host tempo",
                            style = MaterialTheme.typography.labelMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                        )
                    }
                    // Connection indicator
                    Surface(
                        shape = RoundedCornerShape(12.dp),
                        color = if (isConnected)
                            MaterialTheme.colorScheme.primary.copy(alpha = 0.2f)
                        else
                            MaterialTheme.colorScheme.error.copy(alpha = 0.2f)
                    ) {
                        Text(
                            text = if (isConnected) "Connected" else "Disconnected",
                            modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp),
                            style = MaterialTheme.typography.labelSmall,
                            color = if (isConnected)
                                MaterialTheme.colorScheme.primary
                            else
                                MaterialTheme.colorScheme.error
                        )
                    }
                }

                Spacer(Modifier.height(16.dp))

                // Frequency response plot
                Card(
                    modifier = Modifier.fillMaxWidth(),
                    colors = CardDefaults.cardColors(
                        containerColor = MaterialTheme.colorScheme.surface
                    ),
                    shape = RoundedCornerShape(12.dp)
                ) {
                    FrequencyResponsePlot(
                        cutoffHz = cutoff,
                        resonance = resonance,
                        modifier = Modifier.fillMaxWidth()
                    )
                }

                Spacer(Modifier.height(24.dp))

                // Controls
                Card(
                    modifier = Modifier.fillMaxWidth(),
                    colors = CardDefaults.cardColors(
                        containerColor = MaterialTheme.colorScheme.surface
                    ),
                    shape = RoundedCornerShape(12.dp)
                ) {
                    Column(modifier = Modifier.padding(vertical = 8.dp)) {
                        ParamSlider(
                            label = "Cutoff Frequency",
                            value = cutoff,
                            onValueChange = { newVal ->
                                localCutoff = newVal
                                client.setParam("cutoff", newVal)
                            },
                            range = 20f..20000f,
                            logarithmic = true,
                            valueDisplay = { v ->
                                if (v >= 1000f) "%.1f kHz".format(v / 1000f)
                                else "${v.roundToInt()} Hz"
                            },
                            onGestureStart = { client.beginGesture("cutoff") },
                            onGestureEnd = { client.endGesture("cutoff") }
                        )

                        HorizontalDivider(
                            modifier = Modifier.padding(horizontal = 16.dp),
                            color = MaterialTheme.colorScheme.outlineVariant
                        )

                        ParamSlider(
                            label = "Resonance",
                            value = resonance,
                            onValueChange = { newVal ->
                                localResonance = newVal
                                client.setParam("resonance", newVal)
                            },
                            range = 0f..1f,
                            logarithmic = false,
                            valueDisplay = { "%.2f".format(it) },
                            onGestureStart = { client.beginGesture("resonance") },
                            onGestureEnd = { client.endGesture("resonance") }
                        )

                        HorizontalDivider(
                            modifier = Modifier.padding(horizontal = 16.dp),
                            color = MaterialTheme.colorScheme.outlineVariant
                        )

                        ParamSlider(
                            label = "Tempo Sweep (bar-synced)",
                            value = sweep,
                            onValueChange = { newVal ->
                                localSweep = newVal
                                client.setParam("sweep", newVal)
                            },
                            range = 0f..1f,
                            logarithmic = false,
                            valueDisplay = { if (it <= 0.001f) "Off" else "%.2f".format(it) },
                            onGestureStart = { client.beginGesture("sweep") },
                            onGestureEnd = { client.endGesture("sweep") }
                        )
                    }
                }
            }
        }
    }
}
