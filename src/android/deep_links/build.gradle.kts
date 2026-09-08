import org.gradle.api.tasks.bundling.AbstractArchiveTask

plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "dev.dioxus.g3_native_plugins.deep_links"
    compileSdk = 34

    defaultConfig {
        minSdk = 24
        targetSdk = 34
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
    archiveBaseName.set("dx-native-deep-links-plugin")
}

dependencies {
    // ComponentActivity.addOnNewIntentListener, for links that arrive while the
    // app is already running. Activity.onNewIntent is a lifecycle method a
    // library cannot override from outside; the listener is the supported way
    // in. Needs androidx.activity 1.6 or newer.
    //
    // compileOnly because the host activity is an AppCompatActivity and so
    // already brings androidx.activity with it; packaging a second copy in the
    // plugin's own archive only risks a version clash.
    compileOnly("androidx.activity:activity-ktx:1.9.0")
}
