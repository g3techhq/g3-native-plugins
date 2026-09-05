import org.gradle.api.tasks.bundling.AbstractArchiveTask

plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "dev.dioxus.g3_native_plugins.back_button"
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
    archiveBaseName.set("dx-native-back-button-plugin")
}

dependencies {
    // OnBackPressedDispatcher. The platform's deprecated Activity.onBackPressed
    // is not usable on newer Android, where predictive back requires a
    // registered callback.
    //
    // compileOnly because the host activity is an AppCompatActivity and so
    // already brings androidx.activity with it; packaging a second copy in the
    // plugin's own archive only risks a version clash.
    compileOnly("androidx.activity:activity-ktx:1.9.0")
}
