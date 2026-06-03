package com.composevst.components

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.layout.*
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import kotlin.math.cos
import kotlin.math.exp
import kotlin.math.ln
import kotlin.math.sin

/**
 * A rotary knob. Drag vertically to change the value; emits begin/end gesture
 * callbacks around a drag so the host records it as a single automation gesture.
 */
@Composable
fun Knob(
    label: String,
    value: Float,
    onValueChange: (Float) -> Unit,
    range: ClosedFloatingPointRange<Float>,
    modifier: Modifier = Modifier,
    logarithmic: Boolean = false,
    valueDisplay: (Float) -> String = { "%.2f".format(it) },
    onGestureStart: () -> Unit = {},
    onGestureEnd: () -> Unit = {},
) {
    // Read the latest value inside the long-lived drag lambda.
    val currentValue by rememberUpdatedState(value)
    val norm = toNorm(value, range, logarithmic)
    val accent = MaterialTheme.colorScheme.primary
    val track = MaterialTheme.colorScheme.surfaceVariant

    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        modifier = modifier.width(84.dp).padding(vertical = 8.dp)
    ) {
        Text(
            label,
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurface,
            textAlign = TextAlign.Center,
            maxLines = 1,
        )
        Spacer(Modifier.height(6.dp))
        Canvas(
            modifier = Modifier.size(60.dp).pointerInput(range, logarithmic) {
                detectDragGestures(
                    onDragStart = { onGestureStart() },
                    onDragEnd = { onGestureEnd() },
                    onDragCancel = { onGestureEnd() },
                ) { change, drag ->
                    change.consume()
                    val cur = toNorm(currentValue, range, logarithmic)
                    val next = (cur - drag.y / 180f).coerceIn(0f, 1f)
                    onValueChange(fromNorm(next, range, logarithmic))
                }
            }
        ) {
            val stroke = 6.dp.toPx()
            val startAngle = 135f
            val sweep = 270f
            val d = size.minDimension - stroke
            val topLeft = Offset((size.width - d) / 2f, (size.height - d) / 2f)
            val arcSize = Size(d, d)

            drawArc(track, startAngle, sweep, false, topLeft, arcSize, style = Stroke(stroke, cap = StrokeCap.Round))
            drawArc(accent, startAngle, sweep * norm, false, topLeft, arcSize, style = Stroke(stroke, cap = StrokeCap.Round))

            val angle = Math.toRadians((startAngle + sweep * norm).toDouble())
            val r = d / 2f
            val cx = size.width / 2f
            val cy = size.height / 2f
            drawLine(
                accent,
                Offset(cx, cy),
                Offset(cx + r * cos(angle).toFloat(), cy + r * sin(angle).toFloat()),
                strokeWidth = stroke * 0.6f,
                cap = StrokeCap.Round,
            )
        }
        Spacer(Modifier.height(6.dp))
        Text(
            valueDisplay(value),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 1,
        )
    }
}

private fun toNorm(value: Float, range: ClosedFloatingPointRange<Float>, log: Boolean): Float {
    return if (log) {
        val lo = ln(range.start.toDouble())
        val hi = ln(range.endInclusive.toDouble())
        (((ln(value.coerceIn(range).toDouble()) - lo) / (hi - lo)).toFloat()).coerceIn(0f, 1f)
    } else {
        val span = range.endInclusive - range.start
        if (span > 0f) ((value - range.start) / span).coerceIn(0f, 1f) else 0f
    }
}

private fun fromNorm(n: Float, range: ClosedFloatingPointRange<Float>, log: Boolean): Float {
    val v = if (log) {
        val lo = ln(range.start.toDouble())
        val hi = ln(range.endInclusive.toDouble())
        exp(lo + n * (hi - lo)).toFloat()
    } else {
        range.start + n * (range.endInclusive - range.start)
    }
    return v.coerceIn(range.start, range.endInclusive)
}
