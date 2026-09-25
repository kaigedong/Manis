plugins {
    id("com.android.application")
}

android {
    namespace = "dev.manis.app"
    compileSdk = 35

    defaultConfig {
        applicationId = "dev.manis.app"
        minSdk = 26
        // Personal sideload builds execute the digest-verified Mihomo core from app-private storage.
        // Android blocks that for targetSdk >= 29; store builds must bundle the core in the APK.
        targetSdk = 28
        versionCode = 1
        versionName = "0.1.0"
        ndk {
            abiFilters += "arm64-v8a"
        }
        manifestPlaceholders["nativeLibraryName"] = "manis_android"
    }

    buildTypes {
        debug {
            isJniDebuggable = true
        }
        release {
            isMinifyEnabled = false
        }
    }

    sourceSets.getByName("main").jniLibs.srcDir("src/main/jniLibs")
}
