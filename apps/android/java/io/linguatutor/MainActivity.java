package io.linguatutor;

import android.app.Activity;
import android.graphics.Insets;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.content.Intent;
import android.net.Uri;
import android.util.Log;
import android.view.WindowInsets;
import android.webkit.WebView;
import android.webkit.CookieManager;
import java.util.UUID;
import android.webkit.WebViewClient;
import android.webkit.WebResourceRequest;
import android.widget.FrameLayout;
import android.widget.TextView;
import android.widget.Toast;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.HttpURLConnection;
import java.net.URL;

public final class MainActivity extends Activity {
    static { System.loadLibrary("lingua_tutor_android"); }
    private static native String nativeRun(String directory, String token);
    private static native int nativePort();
    private static Thread runtime;
    private static volatile String failure;
    private static volatile String session;
    private final Handler handler = new Handler(Looper.getMainLooper());
    private FrameLayout content;
    private WebView web;
    private String origin;
    private String exportUrl;
    private static final int EXPORT = 10;

    private static synchronized void ensureRuntime(String directory) {
        if (runtime != null && runtime.isAlive()) return;
        failure = null;
        session = (UUID.randomUUID().toString() + UUID.randomUUID().toString()).replace("-", "");
        runtime = new Thread(() -> {
            try {
                failure = nativeRun(directory, session);
                if (failure == null) failure = "Local runtime stopped";
            } catch (Throwable error) {
                failure = "Local runtime failed to start";
                Log.e("LinguaTutor", failure, error);
            }
        }, "lingua-native");
        runtime.start();
    }

    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        content = new FrameLayout(this);
        if (Build.VERSION.SDK_INT >= 35) {
            // Keep the WebView viewport inside system bars, cutouts, and the keyboard.
            content.setOnApplyWindowInsetsListener((view, insets) -> {
                Insets padding = insets.getInsets(WindowInsets.Type.systemBars()
                    | WindowInsets.Type.displayCutout() | WindowInsets.Type.ime());
                view.setPadding(padding.left, padding.top, padding.right, padding.bottom);
                return WindowInsets.CONSUMED;
            });
        }
        TextView loading = new TextView(this);
        loading.setText("Starting Lingua Tutor…");
        loading.setPadding(24, 48, 24, 24);
        content.addView(loading);
        setContentView(content);
        ensureRuntime(getFilesDir().getAbsolutePath());
        awaitRuntime(loading, 0);
    }

    private void awaitRuntime(TextView loading, int attempt) {
        if (isFinishing() || isDestroyed()) return;
        int port = nativePort();
        if (port != 0) {
            showWeb(port);
        } else if (failure != null || attempt >= 300) {
            loading.setText(failure != null ? failure : "Local runtime did not start. Reopen the app.");
        } else {
            handler.postDelayed(() -> awaitRuntime(loading, attempt + 1), 100);
        }
    }

    private boolean local(Uri uri) {
        return origin != null && "http".equals(uri.getScheme())
            && "127.0.0.1".equals(uri.getHost())
            && uri.getPort() == nativePort() && uri.getUserInfo() == null;
    }

    private void showWeb(int port) {
        origin = "http://127.0.0.1:" + port;
        web = new WebView(this);
        web.getSettings().setJavaScriptEnabled(true);
        web.getSettings().setDomStorageEnabled(true);
        web.getSettings().setAllowFileAccess(false);
        web.getSettings().setAllowContentAccess(false);
        web.setWebViewClient(new WebViewClient() {
            @Override public boolean shouldOverrideUrlLoading(WebView view, WebResourceRequest request) {
                Uri uri = request.getUrl();
                if (local(uri)) return false;
                if (request.isForMainFrame() && "https".equals(uri.getScheme())) {
                    try { startActivity(new Intent(Intent.ACTION_VIEW, uri)); }
                    catch (Exception ignored) { Toast.makeText(MainActivity.this, "No browser available", Toast.LENGTH_SHORT).show(); }
                }
                return true;
            }
        });
        web.setDownloadListener((url, userAgent, disposition, mime, length) -> {
            Uri uri = Uri.parse(url);
            String path = uri.getPath();
            if (!local(uri) || !("/api/v1/export/full".equals(path) || "/api/v1/export/cards.csv".equals(path))) return;
            exportUrl = url;
            boolean csv = path.endsWith(".csv");
            Intent save = new Intent(Intent.ACTION_CREATE_DOCUMENT);
            save.addCategory(Intent.CATEGORY_OPENABLE);
            save.setType(csv ? "text/csv" : "application/json");
            save.putExtra(Intent.EXTRA_TITLE, csv ? "cards.csv" : "lingua-export.json");
            startActivityForResult(save, EXPORT);
        });
        content.removeAllViews();
        content.addView(web);
        CookieManager.getInstance().setCookie(origin, "lingua_session=" + session + "; Path=/; HttpOnly; SameSite=Strict", accepted -> {
            if (Boolean.TRUE.equals(accepted) && !isDestroyed()) {
                web.loadUrl(origin + "/setup");
                Log.i("LinguaTutor", "Local runtime ready");
            } else if (!isDestroyed()) {
                Toast.makeText(this, "Cannot initialize private app session", Toast.LENGTH_LONG).show();
            }
        });
    }

    @Override protected void onActivityResult(int request, int result, Intent data) {
        super.onActivityResult(request, result, data);
        if (request != EXPORT || result != RESULT_OK || data == null || data.getData() == null || exportUrl == null) return;
        final String source = exportUrl;
        final Uri target = data.getData();
        exportUrl = null;
        new Thread(() -> {
            HttpURLConnection connection = null;
            try {
                if (!local(Uri.parse(source))) throw new IllegalStateException("Runtime changed");
                connection = (HttpURLConnection) new URL(source).openConnection();
                connection.setInstanceFollowRedirects(false);
                connection.setRequestProperty("Cookie", "lingua_session=" + session);
                connection.setConnectTimeout(10000);
                connection.setReadTimeout(30000);
                if (connection.getResponseCode() != 200) throw new IllegalStateException("Export failed");
                try (InputStream in = connection.getInputStream(); OutputStream out = getContentResolver().openOutputStream(target)) {
                    if (out == null) throw new IllegalStateException("Cannot open export destination");
                    byte[] buffer = new byte[8192];
                    int count;
                    while ((count = in.read(buffer)) != -1) out.write(buffer, 0, count);
                }
                runOnUiThread(() -> Toast.makeText(this, "Export saved", Toast.LENGTH_SHORT).show());
            } catch (Exception error) {
                runOnUiThread(() -> Toast.makeText(this, "Export failed", Toast.LENGTH_SHORT).show());
            } finally { if (connection != null) connection.disconnect(); }
        }, "lingua-export").start();
    }

    @Override public void onBackPressed() {
        if (web != null && web.canGoBack()) web.goBack(); else super.onBackPressed();
    }

    @Override protected void onDestroy() {
        handler.removeCallbacksAndMessages(null);
        if (web != null) web.destroy();
        super.onDestroy();
    }
}
