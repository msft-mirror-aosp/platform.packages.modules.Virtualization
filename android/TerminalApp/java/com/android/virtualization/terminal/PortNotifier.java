/*
 * Copyright (C) 2024 The Android Open Source Project
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *      http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

package com.android.virtualization.terminal;

import static com.android.virtualization.terminal.MainActivity.TAG;

import android.app.Notification;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.SharedPreferences;
import android.graphics.drawable.Icon;
import android.util.Log;

import java.util.HashSet;
import java.util.Locale;
import java.util.Set;

/**
 * PortNotifier is responsible for posting a notification when a new open port is detected. User can
 * enable or disable forwarding of the port in notification panel.
 */
class PortNotifier {
    private final Context mContext;
    private final NotificationManager mNotificationManager;
    private final BroadcastReceiver mReceiver;

    public PortNotifier(Context context) {
        mContext = context;
        mNotificationManager = mContext.getSystemService(NotificationManager.class);
        mReceiver = new PortForwardingRequestReceiver();

        IntentFilter intentFilter =
                new IntentFilter(PortForwardingRequestReceiver.ACTION_PORT_FORWARDING);
        mContext.registerReceiver(mReceiver, intentFilter, Context.RECEIVER_NOT_EXPORTED);
    }

    public void onActivePortsChanged(Set<String> oldPorts, Set<String> newPorts) {
        Set<String> union = new HashSet<>(oldPorts);
        union.addAll(newPorts);
        for (String portStr : union) {
            try {
                if (!oldPorts.contains(portStr)) {
                    showPortForwardingNotification(Integer.parseInt(portStr));
                } else if (!newPorts.contains(portStr)) {
                    discardPortForwardingNotification(Integer.parseInt(portStr));
                }
            } catch (NumberFormatException e) {
                Log.e(TAG, "Failed to parse port: " + portStr);
                throw e;
            }
        }
    }

    public void stop() {
        mContext.unregisterReceiver(mReceiver);
    }

    private String getString(int resId) {
        return mContext.getString(resId);
    }

    private PendingIntent getPortForwardingPendingIntent(int port, boolean enabled) {
        Intent intent = new Intent(PortForwardingRequestReceiver.ACTION_PORT_FORWARDING);
        intent.setPackage(mContext.getPackageName());
        intent.setIdentifier(String.format(Locale.ROOT, "%d_%b", port, enabled));
        intent.putExtra(PortForwardingRequestReceiver.KEY_PORT, port);
        intent.putExtra(PortForwardingRequestReceiver.KEY_ENABLED, enabled);
        return PendingIntent.getBroadcast(mContext, 0, intent, PendingIntent.FLAG_IMMUTABLE);
    }

    private void showPortForwardingNotification(int port) {
        Intent tapIntent = new Intent(mContext, SettingsPortForwardingActivity.class);
        tapIntent.setFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP | Intent.FLAG_ACTIVITY_CLEAR_TOP);
        PendingIntent tapPendingIntent =
                PendingIntent.getActivity(mContext, 0, tapIntent, PendingIntent.FLAG_IMMUTABLE);

        String title = getString(R.string.settings_port_forwarding_notification_title);
        String content =
                mContext.getString(R.string.settings_port_forwarding_notification_content, port);
        String acceptText = getString(R.string.settings_port_forwarding_notification_accept);
        String denyText = getString(R.string.settings_port_forwarding_notification_deny);
        Icon icon = Icon.createWithResource(mContext, R.drawable.ic_launcher_foreground);

        Notification notification =
                new Notification.Builder(mContext, mContext.getPackageName())
                        .setSmallIcon(R.drawable.ic_launcher_foreground)
                        .setContentTitle(title)
                        .setContentText(content)
                        .setContentIntent(tapPendingIntent)
                        .addAction(
                                new Notification.Action.Builder(
                                                icon,
                                                acceptText,
                                                getPortForwardingPendingIntent(
                                                        port, true /* enabled */))
                                        .build())
                        .addAction(
                                new Notification.Action.Builder(
                                                icon,
                                                denyText,
                                                getPortForwardingPendingIntent(
                                                        port, false /* enabled */))
                                        .build())
                        .build();
        mNotificationManager.notify(TAG, port, notification);
    }

    private void discardPortForwardingNotification(int port) {
        mNotificationManager.cancel(TAG, port);
    }

    private final class PortForwardingRequestReceiver extends BroadcastReceiver {
        private static final String ACTION_PORT_FORWARDING =
                "android.virtualization.PORT_FORWARDING";
        private static final String KEY_PORT = "port";
        private static final String KEY_ENABLED = "enabled";

        @Override
        public void onReceive(Context context, Intent intent) {
            if (ACTION_PORT_FORWARDING.equals(intent.getAction())) {
                performActionPortForwarding(context, intent);
            }
        }

        private void performActionPortForwarding(Context context, Intent intent) {
            int port = intent.getIntExtra(KEY_PORT, 0);
            boolean enabled = intent.getBooleanExtra(KEY_ENABLED, false);

            SharedPreferences sharedPref =
                    context.getSharedPreferences(
                            context.getString(R.string.preference_file_key), Context.MODE_PRIVATE);
            SharedPreferences.Editor editor = sharedPref.edit();
            editor.putBoolean(
                    context.getString(R.string.preference_forwarding_port_is_enabled)
                            + Integer.toString(port),
                    enabled);
            editor.apply();

            mNotificationManager.cancel(TAG, port);
        }
    }
}
