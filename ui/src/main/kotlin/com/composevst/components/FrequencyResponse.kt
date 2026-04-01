package com.composevst.components

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.dp
import kotlin.math.*

/**
 * Compute biquad LPF magnitude response (dB) at logarithmically-spaced frequencies.
 */
private fun computeResponse(cutoffHz: Float, resonance: Float, sampleRate: Float = 44100f): List<Pair<Float, Float>> {
    val q = 0.5f + resonance * 9.5f
    val w0 = 2f * PI.toFloat() * cutoffHz / sampleRate
    val cosW0 = cos(w0)
    val sinW0 = sin(w0)
    val alpha = sinW0 / (2f * q)

    val b0 = (1f - cosW0) / 2f
    val b1 = 1f - cosW0
    val b2 = (1f - cosW0) / 2f
    val a0 = 1f + alpha
    val a1 = -2f * cosW0
    val a2 = 1f - alpha

    val b0n = b0 / a0; val b1n = b1 / a0; val b2n = b2 / a0
    val a1n = a1 / a0; val a2n = a2 / a0

    val points = mutableListOf<Pair<Float, Float>>()
    val numPoints = 200
    for (i in 0 until numPoints) {
        val freq = 20f * (20000f / 20f).pow(i.toFloat() / (numPoints - 1))
        val w = 2f * PI.toFloat() * freq / sampleRate
        val cosW = cos(w); val cos2W = cos(2f * w)
        val sinW = sin(w); val sin2W = sin(2f * w)

        val numRe = b0n + b1n * cosW + b2n * cos2W
        val numIm = -(b1n * sinW + b2n * sin2W)
        val denRe = 1f + a1n * cosW + a2n * cos2W
        val denIm = -(a1n * sinW + a2n * sin2W)

        val numMagSq = numRe * numRe + numIm * numIm
        val denMagSq = denRe * denRe + denIm * denIm
        val mag = sqrt(numMagSq / denMagSq)
        val db = 20f * log10(mag.coerceAtLeast(1e-10f))

        points.add(freq to db)
    }
    return points
}

@Composable
fun FrequencyResponsePlot(
    cutoffHz: Float,
    resonance: Float,
    modifier: Modifier = Modifier
) {
    val surfaceVariant = MaterialTheme.colorScheme.surfaceVariant
    val primary = MaterialTheme.colorScheme.primary
    val outline = MaterialTheme.colorScheme.outline.copy(alpha = 0.3f)
    val onSurface = MaterialTheme.colorScheme.onSurface

    Column(modifier = modifier.padding(16.dp)) {
        Text(
            "Frequency Response",
            style = MaterialTheme.typography.labelLarge,
            color = onSurface
        )
        Spacer(Modifier.height(8.dp))
        Canvas(
            modifier = Modifier
                .fillMaxWidth()
                .height(200.dp)
                .clip(RoundedCornerShape(8.dp))
                .background(surfaceVariant)
        ) {
            val w = size.width
            val h = size.height
            val padding = 4f

            val dbMin = -60f
            val dbMax = 20f
            val freqMin = 20f
            val freqMax = 20000f
            val logMin = ln(freqMin)
            val logMax = ln(freqMax)

            // Grid lines (dB)
            for (db in listOf(-48f, -36f, -24f, -12f, 0f, 12f)) {
                val y = h - padding - ((db - dbMin) / (dbMax - dbMin)) * (h - 2 * padding)
                drawLine(outline, Offset(padding, y), Offset(w - padding, y), strokeWidth = 1f)
            }

            // Grid lines (freq)
            for (freq in listOf(100f, 1000f, 10000f)) {
                val x = padding + ((ln(freq) - logMin) / (logMax - logMin)) * (w - 2 * padding)
                drawLine(outline, Offset(x, padding), Offset(x, h - padding), strokeWidth = 1f)
            }

            // Compute and draw response curve
            val points = computeResponse(cutoffHz, resonance)
            val path = Path()
            var first = true

            for ((freq, db) in points) {
                val x = padding + ((ln(freq) - logMin) / (logMax - logMin)) * (w - 2 * padding)
                val clampedDb = db.coerceIn(dbMin, dbMax)
                val y = h - padding - ((clampedDb - dbMin) / (dbMax - dbMin)) * (h - 2 * padding)

                if (first) {
                    path.moveTo(x, y)
                    first = false
                } else {
                    path.lineTo(x, y)
                }
            }

            drawPath(path, primary, style = Stroke(width = 2.5f))
        }
    }
}
