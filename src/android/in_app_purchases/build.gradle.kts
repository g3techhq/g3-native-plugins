import org.gradle.api.tasks.bundling.AbstractArchiveTask

plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "dev.dioxus.g3_native_plugins.in_app_purchases"
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
    archiveBaseName.set("dx-native-in-app-purchases-plugin")
}

dependencies {
    // Not compileOnly, unlike the androidx dependencies elsewhere in this
    // crate: the host app already carries androidx, but it has no reason to
    // carry the billing library. There is also no alternative to weigh here
    // the way LocationManager stands in for the fused location provider —
    // Google Play billing is only reachable through this library.
    //
    // Version 7 is the floor Google requires of new submissions; check the
    // current deadline before shipping, because they move it.
    implementation("com.android.billingclient:billing-ktx:7.1.1")
}
