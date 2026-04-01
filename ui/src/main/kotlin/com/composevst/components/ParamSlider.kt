package com.composevst.components

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import kotlin.math.ln
import kotlin.math.exp
import kotlin.math.roundToInt

@Composable
fun ParamSlider(
    label: String,
    value: Float,
    onValueChange: (Float) -> Unit,
    range: ClosedFloatingPointRange<Float>,
    logarithmic: Boolean = false,
    valueDisplay: (Float) -> String = { "%.2f".format(it) },
    modifier: Modifier = Modifier
) {
    // For logarithmic sliders, map to/from log space
    val sliderValue = if (logarithmic) {
        val logMin = ln(range.start.toDouble())
        val logMax = ln(range.endInclusive.toDouble())
        val logVal = ln(value.coerceIn(range).toDouble())
        ((logVal - logMin) / (logMax - logMin)).toFloat()
    } else {
        val span = range.endInclusive - range.start
        if (span > 0f) (value - range.start) / span else 0f
    }

    Column(modifier = modifier.padding(horizontal = 16.dp, vertical = 8.dp)) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically
        ) {
            Text(
                text = label,
                style = MaterialTheme.typography.labelLarge,
                color = MaterialTheme.colorScheme.onSurface
            )
            Text(
                text = valueDisplay(value),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
        }
        Spacer(Modifier.height(4.dp))
        Slider(
            value = sliderValue.coerceIn(0f, 1f),
            onValueChange = { normalized ->
                val newValue = if (logarithmic) {
                    val logMin = ln(range.start.toDouble())
                    val logMax = ln(range.endInclusive.toDouble())
                    exp(logMin + normalized * (logMax - logMin)).toFloat()
                } else {
                    range.start + normalized * (range.endInclusive - range.start)
                }
                onValueChange(newValue.coerceIn(range))
            },
            modifier = Modifier.fillMaxWidth(),
            colors = SliderDefaults.colors(
                thumbColor = MaterialTheme.colorScheme.primary,
                activeTrackColor = MaterialTheme.colorScheme.primary
            )
        )
    }
}
