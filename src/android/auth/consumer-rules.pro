# JNI loads this bridge by its stable class and method names: no Java or
# Kotlin code references it, so without this rule R8 strips or renames it
# out of a consuming app's release build and loadClass fails on device.
-keep class dev.dioxus.g3_native_plugins.auth.AuthPlugin { *; }
