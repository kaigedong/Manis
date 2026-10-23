package dev.manis.app

import android.app.Application
import dev.manis.app.data.AppContext
import dev.manis.app.data.Workspace

class ManisApp : Application() {
    override fun onCreate() {
        super.onCreate()
        AppContext.install(this)
        Workspace.load(this)
    }
}
