export function Login() {
  const [password, setPassword] = useState("");
  return <input type="password" value={password} onChange={e => setPassword(e.target.value)} />;
}
