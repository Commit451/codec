package com.composevst.components

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp

/** A toggle "button" that reflects an on/off state. */
@Composable
fun ToggleButton(
    label: String,
    active: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val bg = if (active) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.surfaceVariant
    val fg = if (active) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.onSurfaceVariant
    Surface(color = bg, shape = RoundedCornerShape(8.dp), modifier = modifier.clickable { onClick() }) {
        Text(
            label,
            color = fg,
            modifier = Modifier.padding(horizontal = 18.dp, vertical = 10.dp),
            style = MaterialTheme.typography.labelLarge,
        )
    }
}

val DIVISIONS = listOf("1/1", "1/2", "1/4", "1/4T", "1/8", "1/8T", "1/16", "1/32")

/** Segmented selector for the tempo-sync note division (index matches the plugin). */
@Composable
fun DivisionSelector(
    selected: Int,
    enabled: Boolean,
    onSelect: (Int) -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(modifier = modifier, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        DIVISIONS.forEachIndexed { i, name ->
            val isSel = i == selected
            val bg = when {
                isSel && enabled -> MaterialTheme.colorScheme.primary
                isSel -> MaterialTheme.colorScheme.primary.copy(alpha = 0.4f)
                else -> MaterialTheme.colorScheme.surfaceVariant
            }
            val fg = if (isSel) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.onSurfaceVariant
            Surface(
                color = bg,
                shape = RoundedCornerShape(6.dp),
                modifier = Modifier.clickable(enabled = enabled) { onSelect(i) },
            ) {
                Text(
                    name,
                    color = fg,
                    modifier = Modifier.padding(horizontal = 8.dp, vertical = 6.dp),
                    style = MaterialTheme.typography.labelSmall,
                )
            }
        }
    }
}

/** Horizontal output level meter, 0..1. */
@Composable
fun LevelMeter(level: Float, modifier: Modifier = Modifier) {
    Box(
        modifier = modifier
            .height(8.dp)
            .clip(RoundedCornerShape(4.dp))
            .background(MaterialTheme.colorScheme.surfaceVariant)
    ) {
        Box(
            modifier = Modifier
                .fillMaxWidth(level.coerceIn(0f, 1f))
                .fillMaxHeight()
                .background(MaterialTheme.colorScheme.primary)
        )
    }
}
