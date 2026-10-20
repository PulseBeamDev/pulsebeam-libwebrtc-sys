package org.pulsebeam;

import android.app.Activity;
import android.app.Instrumentation;
import android.os.Bundle;

/** Test-only entry point. No camera, microphone, renderer, or device backend. */
public final class Headless extends Instrumentation {
    static { System.loadLibrary("pulsebeam_headless"); }
    private static native int runNative();
    private Bundle arguments;

    @Override public void onCreate(Bundle arguments) {
        super.onCreate(arguments);
        this.arguments = arguments;
        start();
    }

    @Override public void onStart() {
        Bundle result = new Bundle();
        int status = runNative();
        result.putString("pulsebeam_nonce", arguments.getString("pulsebeam_nonce"));
        result.putString("pulsebeam_archive_sha256", arguments.getString("pulsebeam_archive_sha256"));
        result.putInt("pulsebeam_status", status);
        finish(status == 0 ? Activity.RESULT_OK : Activity.RESULT_CANCELED, result);
    }
}
