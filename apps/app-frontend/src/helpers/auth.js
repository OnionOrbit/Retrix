/**
 * All theseus API calls return serialized values (both return values and errors);
 * So, for example, addDefaultInstance creates a blank Profile object, where the Rust struct is serialized,
 *  and deserialized into a usable JS object.
 */
import { invoke } from '@tauri-apps/api/core'

// Offline accounts are stored in localStorage under 'offline_accounts'.
function getOfflineAccounts() {
	try {
		return JSON.parse(localStorage.getItem('offline_accounts') || '[]')
	} catch {
		return []
	}
}

function saveOfflineAccounts(accounts) {
	localStorage.setItem('offline_accounts', JSON.stringify(accounts))
}

function getDefaultOfflineUser() {
	try {
		return localStorage.getItem('default_offline_user') || null
	} catch {
		return null
	}
}
function setDefaultOfflineUser(id) {
	try {
		if (id) localStorage.setItem('default_offline_user', id)
	} catch {}
}
function clearDefaultOfflineUser() {
	try {
		localStorage.removeItem('default_offline_user')
	} catch {}
}

/**
 * Add an offline account (username, optional uuid)
 * @param {string} username
 * @param {string} [uuid]
 * @returns {object} The created offline account object
 */
export function add_offline_account(username, uuid) {
	if (!username) throw new Error('Username required')
	const id = uuid || 'offline-' + username + '-' + Math.random().toString(36).slice(2, 10)
	const account = {
		profile: {
			id,
			name: username,
		},
		type: 'offline',
		offline: true,
	}
	const accounts = getOfflineAccounts()
	accounts.push(account)
	saveOfflineAccounts(accounts)
	return account
}

/**
 * Remove an offline account by id
 */
export function remove_offline_account(id) {
	const accounts = getOfflineAccounts().filter(acc => acc.profile.id !== id)
	saveOfflineAccounts(accounts)
	const def = getDefaultOfflineUser()
	if (def === id) clearDefaultOfflineUser()
}

/**
 * List all offline accounts
 */
export function offline_accounts() {
	return getOfflineAccounts()
}

// Example function:
// User goes to auth_url to complete flow, and when completed, authenticate_await_completion() returns the credentials
// export async function authenticate() {
//   const auth_url = await authenticate_begin_flow()
//   console.log(auth_url)
//   await authenticate_await_completion()
// }

/**
 * Authenticate a user with Hydra - part 1.
 * This begins the authentication flow quasi-synchronously.
 *
 * @returns {Promise<DeviceLoginSuccess>} A DeviceLoginSuccess object with two relevant fields:
 * @property {string} verification_uri - The URL to go to complete the flow.
 * @property {string} user_code - The code to enter on the verification_uri page.
 */
export async function login() {
	return await invoke('plugin:auth|login')
}

/**
 * Retrieves the default user
 * @return {Promise<UUID | undefined>}
 */
export async function get_default_user() {
	const offlineDefault = getDefaultOfflineUser()
	if (offlineDefault) return offlineDefault
	return await invoke('plugin:auth|get_default_user')
}

/**
 * Updates the default user
 * @param {UUID} user
 */
export async function set_default_user(user) {
	// If the ID corresponds to an offline account, store it locally and do not call backend
	if (typeof user === 'string' && user.startsWith('offline-')) {
		setDefaultOfflineUser(user)
		return
	}
	// Otherwise, clear any offline default and set online default via backend
	clearDefaultOfflineUser()
	return await invoke('plugin:auth|set_default_user', { user })
}

/**
 * Remove a user account from the database
 * @param {UUID} user
 */
export async function remove_user(user) {
	return await invoke('plugin:auth|remove_user', { user })
}

/**
 * Returns a list of users
 * @returns {Promise<Credential[]>}
 */
export async function users() {
	// Merge online and offline accounts
	const online = await invoke('plugin:auth|get_users')
	const offline = getOfflineAccounts()
	return [...online, ...offline]
}
