# Loaded by name from Rust (register_android_plugin) and via JNI.
-keep class ai.grokvoice.app.VoicePlugin { *; }
-keep class ai.grokvoice.app.KeyDataArgs { *; }
-keep class ai.grokvoice.app.ServiceArgs { *; }
-keep class ai.grokvoice.app.AlarmArgs { *; }
-keep class ai.grokvoice.app.TimerArgs { *; }
-keep class ai.grokvoice.app.ScheduleArgs { *; }
-keep class ai.grokvoice.app.CancelArgs { *; }
-keep class ai.grokvoice.app.RangeArgs { *; }
-keep class ai.grokvoice.app.EventArgs { *; }
-keep class ai.grokvoice.app.EventUpdateArgs { *; }
-keep class ai.grokvoice.app.EventIdArgs { *; }
-keep class ai.grokvoice.app.PhoneActions { *; }
-keep class ai.grokvoice.app.ReminderReceiver { *; }
-keep class ai.grokvoice.app.SpokenNoticeService { *; }
-keep class ai.grokvoice.app.SpokenAlerts { *; }
-keep class ai.grokvoice.app.BootReceiver { *; }
-keep class ai.grokvoice.app.RustBridge {
    native <methods>;
    *;
}
