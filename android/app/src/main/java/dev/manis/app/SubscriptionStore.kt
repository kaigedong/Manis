package dev.manis.app

import android.content.Context

internal object SubscriptionStore {
    private const val PREFERENCES = "manis"
    private const val KEY_URL = "subscription_url"

    fun url(context: Context): String? =
        context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)
            .getString(KEY_URL, null)
            ?.takeIf { it.isNotBlank() }

    fun save(context: Context, url: String) {
        context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)
            .edit()
            .putString(KEY_URL, url.trim())
            .apply()
    }
}
