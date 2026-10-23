plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

val manisAbi = (findProperty("manisAbi") as String?) ?: "arm64-v8a"

android {
    namespace = "dev.manis.app"
    compileSdk = 35

    defaultConfig {
        applicationId = "dev.manis.app"
        minSdk = 26
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0-android"
        ndk { abiFilters += manisAbi }
    }

    signingConfigs {
        getByName("debug") {
            // Committed debug key: keeps sideload updates installable across CI and local builds.
            storeFile = rootProject.file("debug.keystore")
            storePassword = "android"
            keyAlias = "androiddebugkey"
            keyPassword = "android"
        }
    }

    buildTypes {
        release {
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

    buildFeatures {
        compose = true
    }

    packaging {
        jniLibs {
            // Mihomo is executed from nativeLibraryDir, so the loader must extract it instead of
            // mapping it straight out of the APK.
            useLegacyPackaging = true
        }
        resources.excludes += "/META-INF/{AL2.0,LGPL2.1}"
    }
}

dependencies {
    implementation(platform("androidx.compose:compose-bom:2024.10.01"))
    implementation("androidx.activity:activity-compose:1.9.3")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.material:material-icons-core")
    implementation("androidx.core:core-ktx:1.13.1")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.8.7")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.9.0")
}
