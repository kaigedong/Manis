package dev.manis.app.core

/**
 * The only native surface of the Android application.
 *
 * Implemented by `crates/manis-android`; every function throws `IllegalStateException` on failure
 * and never includes subscription credentials in the message.
 */
internal object Native {
    init {
        System.loadLibrary("manis_android")
    }

    external fun renderConfig(
        subscriptionUrl: String,
        mixedPort: Int,
        fileDescriptor: Int,
    ): String

    external fun startCore(
        binary: String,
        dataDirectory: String,
        configPath: String,
        secret: String,
        fileDescriptor: Int,
    ): Int

    external fun stopCore()
}
