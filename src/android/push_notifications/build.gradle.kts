import org.gradle.api.tasks.bundling.AbstractArchiveTask

plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "dev.dioxus.g3_native_plugins.push_notifications"
    compileSdk = 35

    defaultConfig {
        minSdk = 24
        targetSdk = 35
        consumerProguardFiles("consumer-rules.pro")
    }

    buildTypes {
        getByName("release") {
            isMinifyEnabled = false
        }
        getByName("debug") {
            isMinifyEnabled = false
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }
}

tasks.withType<AbstractArchiveTask>().configureEach {
    archiveBaseName.set("dx-native-push-notifications-plugin")
}

dependencies {
    // Firebase Cloud Messaging is the only way a server reaches an Android app
    // that is not running; there is no framework API to fall back on. A real
    // dependency, like Play Billing in the purchases module, and the reason
    // push is its own feature: an app wanting only local notifications does
    // not carry it.
    implementation("com.google.firebase:firebase-messaging:24.1.0")
    // The new-intent listener, for a push tapped while the app is running.
    compileOnly("androidx.activity:activity-ktx:1.9.0")
}
