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

import android.content.Context;
import android.graphics.Rect;
import android.os.Bundle;
import android.text.InputType;
import android.text.TextUtils;
import android.util.AttributeSet;
import android.util.Log;
import android.view.View;
import android.view.View.AccessibilityDelegate;
import android.view.ViewGroup;
import android.view.accessibility.AccessibilityEvent;
import android.view.accessibility.AccessibilityManager;
import android.view.accessibility.AccessibilityManager.AccessibilityStateChangeListener;
import android.view.accessibility.AccessibilityManager.TouchExplorationStateChangeListener;
import android.view.accessibility.AccessibilityNodeInfo;
import android.view.accessibility.AccessibilityNodeInfo.AccessibilityAction;
import android.view.accessibility.AccessibilityNodeProvider;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputConnection;
import android.webkit.WebView;

import java.util.List;

public class TerminalView extends WebView
        implements AccessibilityStateChangeListener, TouchExplorationStateChangeListener {
    // Maximum length of texts the talk back announcements can be. This value is somewhat
    // arbitrarily set. We may want to adjust this in the future.
    private static final int TEXT_TOO_LONG_TO_ANNOUNCE = 200;

    // keyCode 229 means composing text, so get the last character in e.target.value.
    // keycode 64(@)-95(_) is mapped to a ctrl code
    // keycode 97(A)-122(Z) is converted to a small letter, and mapped to ctrl code
    public static final String CTRL_KEY_HANDLER =
            """
javascript: (function() {
  window.term.attachCustomKeyEventHandler((e) => {
      if (window.ctrl) {
          keyCode = e.keyCode;
          if (keyCode === 229) {
              keyCode = e.target.value.charAt(e.target.selectionStart - 1).charCodeAt();
          }
          if (64 <= keyCode && keyCode <= 95) {
              input = String.fromCharCode(keyCode - 64);
          } else if (97 <= keyCode && keyCode <= 122) {
              input = String.fromCharCode(keyCode - 96);
          } else {
              return true;
          }
          if (e.type === 'keyup') {
              window.term.input(input);
              e.target.value = e.target.value.slice(0, -1);
              window.ctrl = false;
          }
          return false;
      } else {
          return true;
      }
  });
})();
""";
    public static final String ENABLE_CTRL_KEY = "javascript:(function(){window.ctrl=true;})();";

    private final AccessibilityManager mA11yManager;

    public TerminalView(Context context, AttributeSet attrs) {
        super(context, attrs);

        mA11yManager = context.getSystemService(AccessibilityManager.class);
        mA11yManager.addTouchExplorationStateChangeListener(this);
        mA11yManager.addAccessibilityStateChangeListener(this);
        adjustToA11yStateChange();
    }

    @Override
    public void onAccessibilityStateChanged(boolean enabled) {
        Log.d(TAG, "accessibility " + enabled);
        adjustToA11yStateChange();
    }

    @Override
    public void onTouchExplorationStateChanged(boolean enabled) {
        Log.d(TAG, "touch exploration " + enabled);
        adjustToA11yStateChange();
    }

    private void adjustToA11yStateChange() {
        if (!mA11yManager.isEnabled()) {
            setFocusable(true);
            return;
        }

        // When accessibility is on, the webview itself doesn't have to be focusable. The (virtual)
        // edittext will be focusable to accept inputs. However, the webview has to be focusable for
        // an accessibility purpose so that users can read the contents in it or scroll the view.
        setFocusable(false);
        setFocusableInTouchMode(true);
    }

    // AccessibilityEvents for WebView are sent directly from WebContentsAccessibilityImpl to the
    // parent of WebView, without going through WebView. So, there's no WebView methods we can
    // override to intercept the event handling process. To work around this, we attach an
    // AccessibilityDelegate to the parent view where the events are sent to. And to guarantee that
    // the parent view exists, wait until the WebView is attached to the window by when the parent
    // must exist.
    private final AccessibilityDelegate mA11yEventFilter =
            new AccessibilityDelegate() {
                @Override
                public boolean onRequestSendAccessibilityEvent(
                        ViewGroup host, View child, AccessibilityEvent e) {
                    // We filter only the a11y events from the WebView
                    if (child != TerminalView.this) {
                        return super.onRequestSendAccessibilityEvent(host, child, e);
                    }
                    final int eventType = e.getEventType();
                    switch (e.getEventType()) {
                            // Skip reading texts that are too long. Right now, ttyd emits entire
                            // text on the terminal to the live region, which is very annoying to
                            // screen reader users.
                        case AccessibilityEvent.TYPE_ANNOUNCEMENT:
                            CharSequence text = e.getText().get(0); // there always is a text
                            if (text.length() >= TEXT_TOO_LONG_TO_ANNOUNCE) {
                                Log.i(TAG, "Announcement skipped because it's too long: " + text);
                                return false;
                            }
                            break;
                    }
                    return super.onRequestSendAccessibilityEvent(host, child, e);
                }
            };

    @Override
    protected void onAttachedToWindow() {
        super.onAttachedToWindow();
        if (mA11yManager.isEnabled()) {
            View parent = (View) getParent();
            parent.setAccessibilityDelegate(mA11yEventFilter);
        }
    }

    private final AccessibilityNodeProvider mA11yNodeProvider =
            new AccessibilityNodeProvider() {

                /** Returns the original NodeProvider that WebView implements. */
                private AccessibilityNodeProvider getParent() {
                    return TerminalView.super.getAccessibilityNodeProvider();
                }

                /** Convenience method for reading a string resource. */
                private String getString(int resId) {
                    return TerminalView.this.getContext().getResources().getString(resId);
                }

                /** Checks if NodeInfo renders an empty line in the terminal. */
                private boolean isEmptyLine(AccessibilityNodeInfo info) {
                    final CharSequence text = info.getText();
                    // Node with no text is not consiered a line. ttyd emits at least one character,
                    // which usually is NBSP.
                    if (text == null) {
                        return false;
                    }
                    for (int i = 0; i < text.length(); i++) {
                        char c = text.charAt(i);
                        // Note: don't use Characters.isWhitespace as it doesn't recognize NBSP as a
                        // whitespace.
                        if (!TextUtils.isWhitespace(c)) {
                            return false;
                        }
                    }
                    return true;
                }

                @Override
                public AccessibilityNodeInfo createAccessibilityNodeInfo(int id) {
                    AccessibilityNodeInfo info = getParent().createAccessibilityNodeInfo(id);
                    if (info == null) {
                        return null;
                    }

                    final String className = info.getClassName().toString();

                    // By default all views except the cursor is not click-able. Other views are
                    // read-only. This ensures that user is not navigated to non-clickable elements
                    // when using switches.
                    if (!"android.widget.EditText".equals(className)) {
                        info.removeAction(AccessibilityAction.ACTION_CLICK);
                    }

                    switch (className) {
                        case "android.webkit.WebView":
                            // There are two NodeInfo objects of class name WebView. The one is the
                            // real WebView whose ID is View.NO_ID as it's at the root of the
                            // virtual view hierarchy. The second one is a virtual view for the
                            // iframe. The latter one's text is set to the command that we give to
                            // ttyd, which is "login -f droid ...". This is an impl detail which
                            // doesn't have to be announced.  Replace the text with "Terminal
                            // display".
                            if (id != View.NO_ID) {
                                info.setText(null);
                                info.setContentDescription(getString(R.string.terminal_display));
                            }

                            // These two lines below are to prevent this WebView element from being
                            // fousable by the screen reader, while allowing any other element in
                            // the WebView to be focusable by the reader. In our case, the EditText
                            // is a117_focusable.
                            info.setScreenReaderFocusable(false);
                            info.addAction(AccessibilityAction.ACTION_ACCESSIBILITY_FOCUS);
                            break;
                        case "android.view.View":
                            // Empty line was announced as "space" (via the NBSP character).
                            // Localize the spoken text.
                            if (isEmptyLine(info)) {
                                info.setContentDescription(getString(R.string.empty_line));
                            }
                            break;
                        case "android.widget.TextView":
                            // There are several TextViews in the terminal, and one of them is an
                            // invisible TextView which seems to be from the <div
                            // class="live-region"> tag. Interestingly, its text is often populated
                            // with the entire text on the screen. Silence this by forcibly setting
                            // the text to null. Note that this TextView is identified by having a
                            // zero width. This certainly is not elegant, but I couldn't find other
                            // options.
                            Rect rect = new Rect();
                            info.getBoundsInScreen(rect);
                            if (rect.width() == 0) {
                                info.setText(null);
                            }
                            info.setScreenReaderFocusable(false);
                            break;
                        case "android.widget.EditText":
                            // This EditText is for the <textarea> accepting user input; the cursor.
                            // ttyd name it as "Terminal input" but it's not i18n'ed. Override it
                            // here for better i18n.
                            info.setText(null);
                            info.setHintText(null);
                            info.setContentDescription(getString(R.string.terminal_input));
                            info.setScreenReaderFocusable(true);
                            info.addAction(AccessibilityAction.ACTION_FOCUS);
                            break;
                    }
                    return info;
                }

                @Override
                public boolean performAction(int id, int action, Bundle arguments) {
                    return getParent().performAction(id, action, arguments);
                }

                @Override
                public void addExtraDataToAccessibilityNodeInfo(
                        int virtualViewId,
                        AccessibilityNodeInfo info,
                        String extraDataKey,
                        Bundle arguments) {
                    getParent()
                            .addExtraDataToAccessibilityNodeInfo(
                                    virtualViewId, info, extraDataKey, arguments);
                }

                @Override
                public List<AccessibilityNodeInfo> findAccessibilityNodeInfosByText(
                        String text, int virtualViewId) {
                    return getParent().findAccessibilityNodeInfosByText(text, virtualViewId);
                }

                @Override
                public AccessibilityNodeInfo findFocus(int focus) {
                    return getParent().findFocus(focus);
                }
            };

    @Override
    public AccessibilityNodeProvider getAccessibilityNodeProvider() {
        AccessibilityNodeProvider p = super.getAccessibilityNodeProvider();
        if (p != null && mA11yManager.isEnabled()) {
            return mA11yNodeProvider;
        }
        return p;
    }

    @Override
    public InputConnection onCreateInputConnection(EditorInfo outAttrs) {
        InputConnection inputConnection = super.onCreateInputConnection(outAttrs);
        if (outAttrs != null) {
            // TODO(b/378642568): consider using InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD
            // here..
            outAttrs.inputType =
                    InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS;
        }
        return inputConnection;
    }
}
