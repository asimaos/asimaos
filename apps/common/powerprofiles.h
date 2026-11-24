#pragma once

#include <QString>
#include <QVariantMap>

// The power mode (power-profiles-daemon, or a service with its interface): power-saver, balanced,
// performance. Works without root for the person at the computer. Asima Manager and Asima Gaming
// both offer it; there is one owner, so they always agree.
namespace PowerProfiles {

// { available, profile, service, path, wake }
QVariantMap read();
// Switches to `profile`; returns a sentence when it didn't work, nothing when it did.
QString set(const QVariantMap &current, const QString &profile);

} // namespace PowerProfiles
