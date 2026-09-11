package org.pulsebeam.webrtcbuild;

import android.app.Activity;
import android.media.MediaCodecInfo;
import android.media.MediaCodecList;
import android.os.Bundle;
import android.util.Log;
import org.webrtc.Camera2Enumerator;
import org.webrtc.DefaultVideoDecoderFactory;
import org.webrtc.DefaultVideoEncoderFactory;
import org.webrtc.HardwareVideoDecoderFactory;
import org.webrtc.HardwareVideoEncoderFactory;
import org.webrtc.PeerConnectionFactory;
import org.webrtc.ScreenCapturerAndroid;
import org.webrtc.VideoCodecInfo;
import org.webrtc.VideoDecoder;
import org.webrtc.VideoDecoderFactory;
import org.webrtc.VideoEncoder;
import org.webrtc.VideoEncoderFactory;
import org.webrtc.audio.JavaAudioDeviceModule;

public final class NativePackageProbe extends Activity {
  private static final String TAG = "PulsebeamNativeProbe";

  private static VideoCodecInfo findH264(VideoCodecInfo[] codecs) {
    for (VideoCodecInfo codec : codecs) {
      if ("H264".equalsIgnoreCase(codec.name)) {
        return codec;
      }
    }
    return null;
  }

  private static boolean mediaCodecReportsH264(boolean encoder) {
    for (MediaCodecInfo codec :
         new MediaCodecList(MediaCodecList.ALL_CODECS).getCodecInfos()) {
      if (codec.isEncoder() != encoder) {
        continue;
      }
      for (String type : codec.getSupportedTypes()) {
        if ("video/avc".equalsIgnoreCase(type)) {
          return true;
        }
      }
    }
    return false;
  }

  private static void verifyMediaCodecH264Factories() {
    VideoEncoderFactory hardwareEncoder =
        new HardwareVideoEncoderFactory(null, false, true);
    VideoDecoderFactory hardwareDecoder = new HardwareVideoDecoderFactory(null);
    VideoEncoderFactory defaultEncoder =
        new DefaultVideoEncoderFactory(null, false, true);
    VideoDecoderFactory defaultDecoder = new DefaultVideoDecoderFactory(null);
    VideoCodecInfo hardwareEncoderH264 =
        findH264(hardwareEncoder.getSupportedCodecs());
    VideoCodecInfo hardwareDecoderH264 =
        findH264(hardwareDecoder.getSupportedCodecs());
    VideoCodecInfo defaultEncoderH264 =
        findH264(defaultEncoder.getSupportedCodecs());
    VideoCodecInfo defaultDecoderH264 =
        findH264(defaultDecoder.getSupportedCodecs());
    if ((defaultEncoderH264 != null) != (hardwareEncoderH264 != null) ||
        (defaultDecoderH264 != null) != (hardwareDecoderH264 != null)) {
      throw new AssertionError(
          "default H.264 advertisement differs from MediaCodec factories");
    }
    if (defaultEncoderH264 != null) {
      VideoEncoder encoder = defaultEncoder.createEncoder(defaultEncoderH264);
      if (encoder == null) {
        throw new AssertionError(
            "advertised MediaCodec H.264 encoder adapter is missing");
      }
      encoder.release();
    }
    if (defaultDecoderH264 != null) {
      VideoDecoder decoder = defaultDecoder.createDecoder(defaultDecoderH264);
      if (decoder == null) {
        throw new AssertionError(
            "advertised MediaCodec H.264 decoder adapter is missing");
      }
      decoder.release();
    }
    Log.i(TAG, "MediaCodec video/avc reported encoder=" +
                   mediaCodecReportsH264(true) +
                   " decoder=" + mediaCodecReportsH264(false) +
                   "; upstream-compatible advertisement encoder=" +
                   (defaultEncoderH264 != null) +
                   " decoder=" + (defaultDecoderH264 != null));
  }

  @Override
  public void onCreate(Bundle state) {
    super.onCreate(state);
    try {
      PeerConnectionFactory.initialize(
          PeerConnectionFactory.InitializationOptions.builder(this)
              .createInitializationOptions());
      JavaAudioDeviceModule audio =
          JavaAudioDeviceModule.builder(this).createAudioDeviceModule();
      new Camera2Enumerator(this).getDeviceNames();
      if (ScreenCapturerAndroid.class.getName().isEmpty()) {
        throw new AssertionError("screen capture integration is missing");
      }
      verifyMediaCodecH264Factories();
      audio.release();
      Log.i(TAG, "PASS native Android factories loaded");
      finish();
    } catch (Throwable failure) {
      Log.e(TAG, "FAIL native Android package", failure);
      throw new RuntimeException(failure);
    }
  }
}
