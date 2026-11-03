package ai.grokvoice.app

import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyInfo
import android.security.keystore.KeyProperties
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.SecretKeyFactory
import javax.crypto.spec.GCMParameterSpec

/**
 * Wraps the Stronghold vault key with a non-exportable AES-256-GCM key that lives in the
 * Android Keystore (StrongBox when the device has one, otherwise the TEE).
 */
object KeystoreVault {
  private const val ALIAS = "grok_voice_vault_key"
  private const val PROVIDER = "AndroidKeyStore"
  private const val TRANSFORMATION = "AES/GCM/NoPadding"
  private const val VERSION: Byte = 1
  private const val IV_LEN = 12

  private fun existingKey(): SecretKey? {
    val ks = KeyStore.getInstance(PROVIDER).apply { load(null) }
    return ks.getKey(ALIAS, null) as? SecretKey
  }

  private fun generate(strongBox: Boolean): SecretKey {
    val builder = KeyGenParameterSpec.Builder(
      ALIAS,
      KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT
    )
      .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
      .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
      .setKeySize(256)
      .setRandomizedEncryptionRequired(true)
    if (strongBox && Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
      builder.setIsStrongBoxBacked(true)
    }
    val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, PROVIDER)
    return try {
      generator.init(builder.build())
      generator.generateKey()
    } catch (e: Exception) {
      if (strongBox) generate(strongBox = false) else throw e
    }
  }

  fun wrap(raw: ByteArray): ByteArray {
    val key = existingKey() ?: generate(strongBox = Build.VERSION.SDK_INT >= Build.VERSION_CODES.P)
    val cipher = Cipher.getInstance(TRANSFORMATION)
    cipher.init(Cipher.ENCRYPT_MODE, key)
    val iv = cipher.iv
    require(iv.size == IV_LEN) { "unexpected GCM IV length ${iv.size}" }
    return byteArrayOf(VERSION) + iv + cipher.doFinal(raw)
  }

  fun unwrap(blob: ByteArray): ByteArray {
    require(blob.size > 1 + IV_LEN && blob[0] == VERSION) { "unknown wrapped key format" }
    val key = existingKey() ?: throw IllegalStateException("Keystore key missing")
    val cipher = Cipher.getInstance(TRANSFORMATION)
    cipher.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(128, blob, 1, IV_LEN))
    return cipher.doFinal(blob, 1 + IV_LEN, blob.size - 1 - IV_LEN)
  }

  @Suppress("DEPRECATION")
  fun securityLevel(): String {
    val key = existingKey() ?: return "none"
    return try {
      val factory = SecretKeyFactory.getInstance(key.algorithm, PROVIDER)
      val info = factory.getKeySpec(key, KeyInfo::class.java) as KeyInfo
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
        when (info.securityLevel) {
          KeyProperties.SECURITY_LEVEL_STRONGBOX -> "strongbox"
          KeyProperties.SECURITY_LEVEL_TRUSTED_ENVIRONMENT -> "tee"
          KeyProperties.SECURITY_LEVEL_SOFTWARE -> "software"
          else -> "hardware"
        }
      } else if (info.isInsideSecureHardware) "hardware" else "software"
    } catch (e: Exception) {
      "unknown"
    }
  }
}
