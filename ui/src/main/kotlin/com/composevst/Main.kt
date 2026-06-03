package com.composevst

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Window
import androidx.compose.ui.window.application
import androidx.compose.ui.window.rememberWindowState
import com.composevst.components.DivisionSelector
import com.composevst.components.Knob
import com.composevst.components.LevelMeter
import com.composevst.components.ToggleButton

fun main() = application {
    val windowState = rememberWindowState(size = DpSize(560.dp, 640.dp))
    Window(
        onCloseRequest = ::exitApplication,
        title = "Codec — Granular",
        state = windowState,
        resizable = true,
    ) {
        CodecApp()
    }
}

@Composable
fun CodecApp() {
    val client = remember { IpcClient() }
    val s by client.state.collectAsState()
    val isConnected by client.connected.collectAsState()
    // Per-param local override held while a knob is being dragged (removed on release),
    // so the knob doesn't jitter against the ~30 Hz state echo.
    val overrides = remember { mutableStateMapOf<String, Float>() }

    LaunchedEffect(Unit) { client.connect() }
    DisposableEffect(Unit) { onDispose { client.disconnect() } }

    MaterialTheme(colorScheme = darkColorScheme()) {
        Surface(modifier = Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .padding(16.dp)
            ) {
                Header(bpm = s.bpm, connected = isConnected)
                Spacer(Modifier.height(12.dp))

                // Activity: output level + grain count.
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        "${s.grains} grains",
                        style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.width(80.dp),
                    )
                    LevelMeter(level = s.level, modifier = Modifier.weight(1f))
                }
                Spacer(Modifier.height(8.dp))

                Card(
                    modifier = Modifier.fillMaxWidth(),
                    colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
                    shape = RoundedCornerShape(12.dp),
                ) {
                    Column(modifier = Modifier.padding(8.dp)) {
                        KnobRow {
                            knob("density", "Density", s.density, 0.5f..150f, client, overrides, true) {
                                "%.1f/s".format(it)
                            }
                            knob("size", "Grain Size", s.size, 5f..500f, client, overrides, true) {
                                "%.0f ms".format(it)
                            }
                            knob("position", "Position", s.position, 0f..1f, client, overrides) {
                                "%.2f".format(it)
                            }
                        }
                        KnobRow {
                            knob("spray", "Spray", s.spray, 0f..1f, client, overrides) { "%.2f".format(it) }
                            knob("pitch", "Pitch", s.pitch, -24f..24f, client, overrides) {
                                "%+.1f st".format(it)
                            }
                            knob("pitch_spread", "Pitch Spr", s.pitchSpread, 0f..1f, client, overrides) {
                                "%.2f".format(it)
                            }
                        }
                        KnobRow {
                            knob("pan_spread", "Pan Spr", s.panSpread, 0f..1f, client, overrides) {
                                "%.2f".format(it)
                            }
                            knob("feedback", "Feedback", s.feedback, 0f..0.95f, client, overrides) {
                                "%.2f".format(it)
                            }
                            knob("mix", "Mix", s.mix, 0f..1f, client, overrides) {
                                "%.0f%%".format(it * 100)
                            }
                        }
                    }
                }

                Spacer(Modifier.height(16.dp))

                // Buttons.
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    ToggleButton("Sync", s.sync, onClick = { toggle(client, "sync", s.sync) })
                    ToggleButton("Reverse", s.reverse, onClick = { toggle(client, "reverse", s.reverse) })
                }

                Spacer(Modifier.height(12.dp))

                Text(
                    "Tempo division",
                    style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Spacer(Modifier.height(6.dp))
                DivisionSelector(
                    selected = s.division,
                    enabled = s.sync,
                    onSelect = { i ->
                        client.beginGesture("division")
                        client.setParam("division", i.toFloat())
                        client.endGesture("division")
                    },
                )
            }
        }
    }
}

@Composable
private fun Header(bpm: Float, connected: Boolean) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column {
            Text(
                "Codec",
                style = MaterialTheme.typography.headlineSmall,
                color = MaterialTheme.colorScheme.onBackground,
            )
            Text(
                "Granular Cloud",
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Text(
                text = if (bpm > 0f) "♩ %.1f".format(bpm) else "no tempo",
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Surface(
                shape = RoundedCornerShape(12.dp),
                color = if (connected) MaterialTheme.colorScheme.primary.copy(alpha = 0.2f)
                else MaterialTheme.colorScheme.error.copy(alpha = 0.2f),
            ) {
                Text(
                    text = if (connected) "Connected" else "Disconnected",
                    modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp),
                    style = MaterialTheme.typography.labelSmall,
                    color = if (connected) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.error,
                )
            }
        }
    }
}

@Composable
private fun KnobRow(content: @Composable RowScope.() -> Unit) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.SpaceEvenly,
        content = content,
    )
}

@Composable
private fun knob(
    name: String,
    label: String,
    stateValue: Float,
    range: ClosedFloatingPointRange<Float>,
    client: IpcClient,
    overrides: MutableMap<String, Float>,
    logarithmic: Boolean = false,
    valueDisplay: (Float) -> String,
) {
    val value = overrides[name] ?: stateValue
    Knob(
        label = label,
        value = value,
        onValueChange = { v ->
            overrides[name] = v
            client.setParam(name, v)
        },
        range = range,
        logarithmic = logarithmic,
        valueDisplay = valueDisplay,
        onGestureStart = { client.beginGesture(name) },
        onGestureEnd = {
            client.endGesture(name)
            overrides.remove(name) // let host/state drive the knob again after release
        },
    )
}

private fun toggle(client: IpcClient, name: String, current: Boolean) {
    val next = !current
    client.beginGesture(name)
    client.setParam(name, if (next) 1f else 0f)
    client.endGesture(name)
}
