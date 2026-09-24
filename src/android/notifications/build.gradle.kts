import org.gradle.api.tasks.bundling.AbstractArchiveTask

plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "dev.dioxus.g3_native_plugins.notifications"
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
    archiveBaseName.set("dx-native-notifications-plugin")
}

dependencies {
    // ComponentActivity.addOnNewIntentListener, for a notification tapped while
    // the app is already running. Activity.onNewIntent is a lifecycle method a
    // library cannot override from outside; the listener is the supported way
    // in. compileOnly because the host activity already brings androidx.activity
    // with it, the same as the deep-links module.
    //
    // Nothing else: notifications, channels, alarms, and RemoteInput are all
    // framework APIs, so there is no NotificationCompat to carry.
    compileOnly("androidx.activity:activity-ktx:1.9.0")
}
