import org.jetbrains.compose.desktop.application.dsl.TargetFormat

plugins {
    kotlin("jvm") version "2.1.0"
    id("org.jetbrains.compose") version "1.7.3"
    id("org.jetbrains.kotlin.plugin.compose") version "2.1.0"
}

group = "com.composevst"
version = "0.1.0"

// Native package formats are validated against the host OS at configuration
// time, so only declare the ones valid for the current platform.
val hostTargetFormats = System.getProperty("os.name").lowercase().let { os ->
    when {
        os.contains("mac") -> arrayOf(TargetFormat.Dmg, TargetFormat.Pkg)
        os.contains("win") -> arrayOf(TargetFormat.Msi, TargetFormat.Exe)
        else -> arrayOf(TargetFormat.Deb, TargetFormat.Rpm, TargetFormat.AppImage)
    }
}

dependencies {
    implementation(compose.desktop.currentOs)
    implementation(compose.material3)
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.9.0")
}

compose.desktop {
    application {
        mainClass = "com.composevst.MainKt"

        nativeDistributions {
            targetFormats(*hostTargetFormats)
            packageName = "compose-vst-ui"
            packageVersion = "1.0.0"
        }
    }
}
